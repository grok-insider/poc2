mod engine;
pub mod protocol;

use std::{
    io::{self, BufRead, Read, Write},
    time::Instant,
};

use engine::{BackendError, BackendErrorKind, OcrBackend};
use protocol::{
    decode_png_base64, parse_request, ErrorCode, ErrorResponse, HelloResponse, Limits,
    ProtocolError, RecognizeResponse, Request, Response, BACKEND_NAME, DEFAULT_LANGUAGE,
    MAX_PNG_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, PROTOCOL_VERSION,
};

pub struct OcrService {
    backend: OcrBackend,
}

impl Default for OcrService {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrService {
    pub fn new() -> Self {
        Self {
            backend: OcrBackend::new(),
        }
    }

    pub fn process_line(&mut self, line: &[u8]) -> Response {
        match parse_request(line).and_then(|request| self.handle(request)) {
            Ok(response) => response,
            Err(error) => Response::Error(error.into()),
        }
    }

    fn handle(&mut self, request: Request) -> Result<Response, ProtocolError> {
        match request {
            Request::Hello { id } => Ok(Response::Hello(HelloResponse {
                version: PROTOCOL_VERSION,
                id,
                response_type: "hello",
                ok: true,
                backend: BACKEND_NAME,
                available: cfg!(windows),
                default_language: DEFAULT_LANGUAGE,
                limits: Limits {
                    max_request_bytes: MAX_REQUEST_BYTES as u64,
                    max_png_bytes: MAX_PNG_BYTES as u64,
                    max_response_bytes: MAX_RESPONSE_BYTES as u64,
                },
            })),
            Request::Recognize {
                id,
                png_base64,
                language,
            } => {
                let started = Instant::now();
                let png = decode_png_base64(&id, &png_base64)?;
                let recognition = self
                    .backend
                    .recognize(&png, &language)
                    .map_err(|error| backend_protocol_error(&id, error))?;
                Ok(Response::Recognize(RecognizeResponse {
                    version: PROTOCOL_VERSION,
                    id,
                    response_type: "recognize",
                    ok: true,
                    backend: BACKEND_NAME,
                    language: recognition.language,
                    image: recognition.image,
                    text: recognition.text,
                    lines: recognition.lines,
                    elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                }))
            }
        }
    }
}

fn backend_protocol_error(id: &str, error: BackendError) -> ProtocolError {
    let code = match error.kind {
        #[cfg(windows)]
        BackendErrorKind::InvalidLanguage => ErrorCode::InvalidLanguage,
        BackendErrorKind::EngineUnavailable => ErrorCode::EngineUnavailable,
        #[cfg(windows)]
        BackendErrorKind::ImageTooLarge => ErrorCode::ImageTooLarge,
        #[cfg(windows)]
        BackendErrorKind::DecodeFailed => ErrorCode::DecodeFailed,
        #[cfg(windows)]
        BackendErrorKind::RecognitionFailed => ErrorCode::RecognitionFailed,
    };
    ProtocolError::new(Some(id.to_owned()), code, error.message)
}

enum BoundedLine {
    Eof,
    Line(Vec<u8>),
    TooLarge,
}

fn read_bounded_line(reader: &mut impl BufRead) -> io::Result<BoundedLine> {
    let mut bytes = Vec::new();
    let count = {
        let mut limited = (&mut *reader).take((MAX_REQUEST_BYTES + 2) as u64);
        limited.read_until(b'\n', &mut bytes)?
    };

    if count == 0 {
        return Ok(BoundedLine::Eof);
    }
    let terminated = bytes.ends_with(b"\n");
    if terminated {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    if bytes.len() > MAX_REQUEST_BYTES {
        if !terminated {
            discard_through_newline(reader)?;
        }
        return Ok(BoundedLine::TooLarge);
    }
    Ok(BoundedLine::Line(bytes))
}

fn discard_through_newline(reader: &mut impl BufRead) -> io::Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        if let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
            reader.consume(position + 1);
            return Ok(());
        }
        let length = buffer.len();
        reader.consume(length);
    }
}

fn write_response(writer: &mut impl Write, response: Response) -> io::Result<()> {
    let response_id = response.id().map(str::to_owned);
    let mut encoded = serde_json::to_vec(&response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if encoded.len() > MAX_RESPONSE_BYTES {
        let fallback = Response::Error(ErrorResponse::from(ProtocolError::new(
            response_id,
            ErrorCode::ResponseTooLarge,
            "response exceeds the NDJSON output limit",
        )));
        encoded = serde_json::to_vec(&fallback)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    }
    encoded.push(b'\n');
    writer.write_all(&encoded)?;
    writer.flush()
}

pub fn run(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<()> {
    let mut service = OcrService::new();
    loop {
        let response = match read_bounded_line(reader)? {
            BoundedLine::Eof => return Ok(()),
            BoundedLine::Line(line) => service.process_line(&line),
            BoundedLine::TooLarge => Response::Error(ErrorResponse::from(ProtocolError::new(
                None,
                ErrorCode::RequestTooLarge,
                "request exceeds the NDJSON input limit",
            ))),
        };
        write_response(writer, response)?;
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use serde_json::{json, Value};

    use super::*;
    use crate::protocol::{union_rectangles, BoundingBox, ErrorCode, Request};

    #[test]
    fn parses_hello_and_recognize_with_default_language() {
        let hello = parse_request(br#"{"version":1,"id":"hello-1","type":"hello"}"#).unwrap();
        assert_eq!(
            hello,
            Request::Hello {
                id: "hello-1".to_owned()
            }
        );

        let recognize = parse_request(
            br#"{"version":1,"id":"scan-1","type":"recognize","pngBase64":"iVBORw0KGgo="}"#,
        )
        .unwrap();
        assert_eq!(
            recognize,
            Request::Recognize {
                id: "scan-1".to_owned(),
                png_base64: "iVBORw0KGgo=".to_owned(),
                language: "en-US".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_non_string_and_oversized_ids() {
        let non_string = parse_request(br#"{"version":1,"id":7,"type":"hello"}"#).unwrap_err();
        assert_eq!(non_string.code, ErrorCode::InvalidId);

        let request = json!({
            "version": 1,
            "id": "x".repeat(crate::protocol::MAX_ID_BYTES + 1),
            "type": "hello"
        });
        let oversized = parse_request(request.to_string().as_bytes()).unwrap_err();
        assert_eq!(oversized.code, ErrorCode::InvalidId);
        assert_eq!(oversized.id, None);
    }

    #[test]
    fn rejects_malformed_json_base64_and_png() {
        let malformed = parse_request(br#"{"version":1"#).unwrap_err();
        assert_eq!(malformed.code, ErrorCode::InvalidJson);

        let invalid_base64 = decode_png_base64("scan", "not base64!").unwrap_err();
        assert_eq!(invalid_base64.code, ErrorCode::InvalidBase64);

        let invalid_png = decode_png_base64("scan", "aGVsbG8=").unwrap_err();
        assert_eq!(invalid_png.code, ErrorCode::InvalidPng);
    }

    #[test]
    fn unions_word_rectangles_and_ignores_invalid_rectangles() {
        let union = union_rectangles([
            BoundingBox::new(10.0, 20.0, 5.0, 7.0),
            BoundingBox::new(4.0, 22.0, 20.0, 3.0),
            BoundingBox::new(f32::NAN, 0.0, 1.0, 1.0),
        ])
        .unwrap();
        assert_eq!(union, BoundingBox::new(4.0, 20.0, 20.0, 7.0));
        assert_eq!(union_rectangles([]), None);
    }

    #[test]
    fn oversized_input_is_discarded_and_stream_processing_continues() {
        let mut input = vec![b'x'; MAX_REQUEST_BYTES + 1];
        input.extend_from_slice(b"\n{\"version\":1,\"id\":\"next\",\"type\":\"hello\"}\n");
        let mut reader = BufReader::new(Cursor::new(input));
        let mut output = Vec::new();

        run(&mut reader, &mut output).unwrap();

        let responses: Vec<Value> = output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], "request-too-large");
        assert_eq!(responses[1]["type"], "hello");
        assert_eq!(responses[1]["id"], "next");
    }

    #[test]
    fn oversized_response_is_replaced_with_bounded_error() {
        let response = Response::Recognize(RecognizeResponse {
            version: PROTOCOL_VERSION,
            id: "large".to_owned(),
            response_type: "recognize",
            ok: true,
            backend: BACKEND_NAME,
            language: DEFAULT_LANGUAGE.to_owned(),
            image: crate::protocol::ImageDimensions {
                width: 1,
                height: 1,
            },
            text: "x".repeat(MAX_RESPONSE_BYTES),
            lines: Vec::new(),
            elapsed_ms: 1,
        });
        let mut output = Vec::new();

        write_response(&mut output, response).unwrap();

        assert!(output.len() <= MAX_RESPONSE_BYTES + 1);
        let value: Value = serde_json::from_slice(&output[..output.len() - 1]).unwrap();
        assert_eq!(value["id"], "large");
        assert_eq!(value["error"]["code"], "response-too-large");
    }

    #[cfg(not(windows))]
    #[test]
    fn recognize_reports_engine_unavailable_off_windows() {
        let mut service = OcrService::new();
        let response = service.process_line(
            br#"{"version":1,"id":"scan-1","type":"recognize","pngBase64":"iVBORw0KGgo="}"#,
        );

        let Response::Error(error) = response else {
            panic!("expected an error response");
        };
        assert_eq!(error.id.as_deref(), Some("scan-1"));
        assert_eq!(error.error.code, ErrorCode::EngineUnavailable);
    }
}
