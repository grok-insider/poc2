use crate::protocol::{ImageDimensions, OcrLine};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendErrorKind {
    #[cfg(windows)]
    InvalidLanguage,
    EngineUnavailable,
    #[cfg(windows)]
    ImageTooLarge,
    #[cfg(windows)]
    DecodeFailed,
    #[cfg(windows)]
    RecognitionFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    pub message: String,
}

impl BackendError {
    fn new(kind: BackendErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawRecognition {
    pub language: String,
    pub image: ImageDimensions,
    pub text: String,
    pub lines: Vec<OcrLine>,
}

#[cfg(not(windows))]
mod platform {
    use super::{BackendError, BackendErrorKind, RawRecognition};

    #[derive(Default)]
    pub struct OcrBackend;

    impl OcrBackend {
        pub fn new() -> Self {
            Self
        }

        pub fn recognize(
            &mut self,
            _png: &[u8],
            _language: &str,
        ) -> Result<RawRecognition, BackendError> {
            Err(BackendError::new(
                BackendErrorKind::EngineUnavailable,
                "Windows.Media.Ocr is available only on Windows",
            ))
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::collections::HashMap;

    use windows::{
        core::{Interface, HSTRING},
        Globalization::Language,
        Graphics::Imaging::BitmapDecoder,
        Media::Ocr::{OcrEngine, OcrResult},
        Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };

    use crate::protocol::{union_rectangles, BoundingBox, ImageDimensions, OcrLine};

    use super::{BackendError, BackendErrorKind, RawRecognition};

    pub struct OcrBackend {
        runtime_initialized: bool,
        engines: HashMap<String, OcrEngine>,
    }

    impl OcrBackend {
        pub fn new() -> Self {
            Self {
                runtime_initialized: false,
                engines: HashMap::new(),
            }
        }

        pub fn recognize(
            &mut self,
            png: &[u8],
            language: &str,
        ) -> Result<RawRecognition, BackendError> {
            self.ensure_runtime()?;
            let (bitmap, image) = decode_png(png)?;
            let engine = self.engine_for_language(language)?;
            let result = engine.RecognizeAsync(&bitmap).map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "could not start OCR recognition",
                    error,
                )
            })?;
            let result = result.get().map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "OCR recognition failed",
                    error,
                )
            })?;
            recognition_result(result, language, image)
        }

        fn ensure_runtime(&mut self) -> Result<(), BackendError> {
            if self.runtime_initialized {
                return Ok(());
            }
            // The helper owns this persistent process thread, so initialize its MTA once
            // rather than adding COM setup and teardown to every recognition request.
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| {
                backend_error(
                    BackendErrorKind::EngineUnavailable,
                    "could not initialize the Windows Runtime",
                    error,
                )
            })?;
            self.runtime_initialized = true;
            Ok(())
        }

        fn engine_for_language(&mut self, language: &str) -> Result<&OcrEngine, BackendError> {
            if !self.engines.contains_key(language) {
                let language_value =
                    Language::CreateLanguage(&HSTRING::from(language)).map_err(|error| {
                        backend_error(
                            BackendErrorKind::InvalidLanguage,
                            "Windows rejected the requested OCR language",
                            error,
                        )
                    })?;
                let engine =
                    OcrEngine::TryCreateFromLanguage(&language_value).map_err(|error| {
                        backend_error(
                            BackendErrorKind::EngineUnavailable,
                            "could not create an OCR engine for the requested language",
                            error,
                        )
                    })?;
                if engine.as_raw().is_null() {
                    return Err(BackendError::new(
                        BackendErrorKind::EngineUnavailable,
                        format!("no installed OCR engine supports language {language}"),
                    ));
                }
                self.engines.insert(language.to_owned(), engine);
            }

            self.engines.get(language).ok_or_else(|| {
                BackendError::new(
                    BackendErrorKind::EngineUnavailable,
                    "OCR engine cache did not retain the requested language",
                )
            })
        }
    }

    impl Drop for OcrBackend {
        fn drop(&mut self) {
            if self.runtime_initialized {
                self.engines.clear();
                unsafe { RoUninitialize() };
            }
        }
    }

    fn decode_png(
        png: &[u8],
    ) -> Result<(windows::Graphics::Imaging::SoftwareBitmap, ImageDimensions), BackendError> {
        let stream = InMemoryRandomAccessStream::new().map_err(|error| {
            backend_error(
                BackendErrorKind::DecodeFailed,
                "could not allocate an in-memory image stream",
                error,
            )
        })?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(|error| {
            backend_error(
                BackendErrorKind::DecodeFailed,
                "could not open the in-memory image stream",
                error,
            )
        })?;
        writer.WriteBytes(png).map_err(|error| {
            backend_error(
                BackendErrorKind::DecodeFailed,
                "could not write PNG bytes to the image stream",
                error,
            )
        })?;
        writer
            .StoreAsync()
            .and_then(|operation| operation.get())
            .map_err(|error| {
                backend_error(
                    BackendErrorKind::DecodeFailed,
                    "could not commit PNG bytes to the image stream",
                    error,
                )
            })?;
        writer.DetachStream().map_err(|error| {
            backend_error(
                BackendErrorKind::DecodeFailed,
                "could not detach the image stream writer",
                error,
            )
        })?;
        stream.Seek(0).map_err(|error| {
            backend_error(
                BackendErrorKind::DecodeFailed,
                "could not rewind the image stream",
                error,
            )
        })?;

        let decoder = BitmapDecoder::CreateAsync(&stream)
            .and_then(|operation| operation.get())
            .map_err(|error| {
                backend_error(
                    BackendErrorKind::DecodeFailed,
                    "Windows could not decode the PNG",
                    error,
                )
            })?;
        let image = ImageDimensions {
            width: decoder.PixelWidth().map_err(|error| {
                backend_error(
                    BackendErrorKind::DecodeFailed,
                    "could not read the decoded image width",
                    error,
                )
            })?,
            height: decoder.PixelHeight().map_err(|error| {
                backend_error(
                    BackendErrorKind::DecodeFailed,
                    "could not read the decoded image height",
                    error,
                )
            })?,
        };
        let max_dimension = OcrEngine::MaxImageDimension().map_err(|error| {
            backend_error(
                BackendErrorKind::EngineUnavailable,
                "could not query the OCR image dimension limit",
                error,
            )
        })?;
        if image.width > max_dimension || image.height > max_dimension {
            return Err(BackendError::new(
                BackendErrorKind::ImageTooLarge,
                format!(
                    "decoded image is {}x{}; Windows.Media.Ocr supports at most {max_dimension}px per dimension",
                    image.width, image.height
                ),
            ));
        }
        let bitmap = decoder
            .GetSoftwareBitmapAsync()
            .and_then(|operation| operation.get())
            .map_err(|error| {
                backend_error(
                    BackendErrorKind::DecodeFailed,
                    "could not materialize the decoded PNG bitmap",
                    error,
                )
            })?;
        Ok((bitmap, image))
    }

    fn recognition_result(
        result: OcrResult,
        language: &str,
        image: ImageDimensions,
    ) -> Result<RawRecognition, BackendError> {
        let text = result.Text().map_err(|error| {
            backend_error(
                BackendErrorKind::RecognitionFailed,
                "could not read OCR text",
                error,
            )
        })?;
        let windows_lines = result.Lines().map_err(|error| {
            backend_error(
                BackendErrorKind::RecognitionFailed,
                "could not read OCR lines",
                error,
            )
        })?;
        let line_count = windows_lines.Size().map_err(|error| {
            backend_error(
                BackendErrorKind::RecognitionFailed,
                "could not count OCR lines",
                error,
            )
        })?;
        let mut lines = Vec::with_capacity(line_count as usize);

        for index in 0..line_count {
            let line = windows_lines.GetAt(index).map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "could not read an OCR line",
                    error,
                )
            })?;
            let line_text = line.Text().map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "could not read OCR line text",
                    error,
                )
            })?;
            let words = line.Words().map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "could not read OCR words",
                    error,
                )
            })?;
            let word_count = words.Size().map_err(|error| {
                backend_error(
                    BackendErrorKind::RecognitionFailed,
                    "could not count OCR words",
                    error,
                )
            })?;
            let mut rectangles = Vec::with_capacity(word_count as usize);
            for word_index in 0..word_count {
                let word = words.GetAt(word_index).map_err(|error| {
                    backend_error(
                        BackendErrorKind::RecognitionFailed,
                        "could not read an OCR word",
                        error,
                    )
                })?;
                let rectangle = word.BoundingRect().map_err(|error| {
                    backend_error(
                        BackendErrorKind::RecognitionFailed,
                        "could not read an OCR word rectangle",
                        error,
                    )
                })?;
                rectangles.push(BoundingBox::new(
                    rectangle.X,
                    rectangle.Y,
                    rectangle.Width,
                    rectangle.Height,
                ));
            }
            lines.push(OcrLine {
                text: line_text.to_string(),
                bbox: union_rectangles(rectangles),
            });
        }

        Ok(RawRecognition {
            language: language.to_owned(),
            image,
            text: text.to_string(),
            lines,
        })
    }

    fn backend_error(
        kind: BackendErrorKind,
        context: &str,
        error: windows::core::Error,
    ) -> BackendError {
        BackendError::new(kind, format!("{context}: {error}"))
    }
}

pub use platform::OcrBackend;
