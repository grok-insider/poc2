use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const BACKEND_NAME: &str = "windows-media-ocr";
pub const DEFAULT_LANGUAGE: &str = "en-US";
pub const MAX_REQUEST_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_PNG_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PNG_BASE64_BYTES: usize = MAX_PNG_BYTES.div_ceil(3) * 4;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_LANGUAGE_BYTES: usize = 64;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Hello {
        id: String,
    },
    Recognize {
        id: String,
        png_base64: String,
        language: String,
    },
}

impl Request {
    pub fn id(&self) -> &str {
        match self {
            Self::Hello { id } | Self::Recognize { id, .. } => id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    InvalidJson,
    InvalidRequest,
    UnsupportedVersion,
    InvalidId,
    RequestTooLarge,
    ImageTooLarge,
    InvalidBase64,
    InvalidPng,
    InvalidLanguage,
    EngineUnavailable,
    DecodeFailed,
    RecognitionFailed,
    ResponseTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub id: Option<String>,
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(id: Option<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id,
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawEnvelope {
    version: u32,
    id: Value,
    #[serde(rename = "type")]
    request_type: String,
    #[serde(rename = "pngBase64")]
    png_base64: Option<String>,
    language: Option<String>,
}

pub fn parse_request(input: &[u8]) -> Result<Request, ProtocolError> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::new(
            None,
            ErrorCode::RequestTooLarge,
            "request exceeds the NDJSON input limit",
        ));
    }

    let value: Value = serde_json::from_slice(input).map_err(|error| {
        ProtocolError::new(
            None,
            ErrorCode::InvalidJson,
            format!("invalid JSON: {error}"),
        )
    })?;
    let id_hint = bounded_id_hint(&value);
    let raw: RawEnvelope = serde_json::from_value(value).map_err(|error| {
        ProtocolError::new(
            id_hint.clone(),
            ErrorCode::InvalidRequest,
            format!("invalid request envelope: {error}"),
        )
    })?;

    let id = validate_id(raw.id)?;
    if raw.version != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            Some(id),
            ErrorCode::UnsupportedVersion,
            format!(
                "unsupported protocol version {}; expected {PROTOCOL_VERSION}",
                raw.version
            ),
        ));
    }

    match raw.request_type.as_str() {
        "hello" => Ok(Request::Hello { id }),
        "recognize" => {
            let png_base64 = raw.png_base64.ok_or_else(|| {
                ProtocolError::new(
                    Some(id.clone()),
                    ErrorCode::InvalidRequest,
                    "recognize requires pngBase64",
                )
            })?;
            if png_base64.len() > MAX_PNG_BASE64_BYTES {
                return Err(ProtocolError::new(
                    Some(id),
                    ErrorCode::ImageTooLarge,
                    "base64 image exceeds the decoded PNG limit",
                ));
            }

            let language = raw.language.unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned());
            validate_language(&id, &language)?;
            Ok(Request::Recognize {
                id,
                png_base64,
                language,
            })
        }
        _ => Err(ProtocolError::new(
            Some(id),
            ErrorCode::InvalidRequest,
            "type must be hello or recognize",
        )),
    }
}

fn bounded_id_hint(value: &Value) -> Option<String> {
    let id = value.get("id")?.as_str()?;
    (is_valid_id(id)).then(|| id.to_owned())
}

fn validate_id(value: Value) -> Result<String, ProtocolError> {
    let Some(id) = value.as_str() else {
        return Err(ProtocolError::new(
            None,
            ErrorCode::InvalidId,
            "id must be a string",
        ));
    };
    if !is_valid_id(id) {
        return Err(ProtocolError::new(
            None,
            ErrorCode::InvalidId,
            format!(
                "id must be non-empty, contain no control characters, and be at most {MAX_ID_BYTES} bytes"
            ),
        ));
    }
    Ok(id.to_owned())
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.chars().any(char::is_control)
}

fn validate_language(id: &str, language: &str) -> Result<(), ProtocolError> {
    let valid = !language.is_empty()
        && language.len() <= MAX_LANGUAGE_BYTES
        && language.is_ascii()
        && !language.starts_with('-')
        && !language.ends_with('-')
        && !language.contains("--")
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');

    if valid {
        Ok(())
    } else {
        Err(ProtocolError::new(
            Some(id.to_owned()),
            ErrorCode::InvalidLanguage,
            "language must be a valid ASCII BCP-47-style tag",
        ))
    }
}

pub fn decode_png_base64(id: &str, encoded: &str) -> Result<Vec<u8>, ProtocolError> {
    if encoded.len() > MAX_PNG_BASE64_BYTES {
        return Err(ProtocolError::new(
            Some(id.to_owned()),
            ErrorCode::ImageTooLarge,
            "base64 image exceeds the decoded PNG limit",
        ));
    }

    let bytes = BASE64_STANDARD.decode(encoded).map_err(|_| {
        ProtocolError::new(
            Some(id.to_owned()),
            ErrorCode::InvalidBase64,
            "pngBase64 is not valid standard base64",
        )
    })?;
    if bytes.len() > MAX_PNG_BYTES {
        return Err(ProtocolError::new(
            Some(id.to_owned()),
            ErrorCode::ImageTooLarge,
            "decoded PNG exceeds the image limit",
        ));
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err(ProtocolError::new(
            Some(id.to_owned()),
            ErrorCode::InvalidPng,
            "decoded image does not have a PNG signature",
        ));
    }
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub fn union_rectangles(rectangles: impl IntoIterator<Item = BoundingBox>) -> Option<BoundingBox> {
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for rectangle in rectangles {
        let right = rectangle.x + rectangle.width;
        let bottom = rectangle.y + rectangle.height;
        if !rectangle.x.is_finite()
            || !rectangle.y.is_finite()
            || !right.is_finite()
            || !bottom.is_finite()
            || rectangle.width < 0.0
            || rectangle.height < 0.0
        {
            continue;
        }

        bounds = Some(match bounds {
            None => (rectangle.x, rectangle.y, right, bottom),
            Some((left, top, old_right, old_bottom)) => (
                left.min(rectangle.x),
                top.min(rectangle.y),
                old_right.max(right),
                old_bottom.max(bottom),
            ),
        });
    }

    bounds.map(|(left, top, right, bottom)| BoundingBox::new(left, top, right - left, bottom - top))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub bbox: Option<BoundingBox>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_request_bytes: u64,
    pub max_png_bytes: u64,
    pub max_response_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: &'static str,
    pub ok: bool,
    pub backend: &'static str,
    pub available: bool,
    pub default_language: &'static str,
    pub limits: Limits,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizeResponse {
    pub version: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: &'static str,
    pub ok: bool,
    pub backend: &'static str,
    pub language: String,
    pub image: ImageDimensions,
    pub text: String,
    pub lines: Vec<OcrLine>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub response_type: &'static str,
    pub ok: bool,
    pub error: ErrorBody,
}

impl From<ProtocolError> for ErrorResponse {
    fn from(error: ProtocolError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id: error.id,
            response_type: "error",
            ok: false,
            error: ErrorBody {
                code: error.code,
                message: error.message,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Response {
    Hello(HelloResponse),
    Recognize(RecognizeResponse),
    Error(ErrorResponse),
}

impl Response {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Hello(response) => Some(&response.id),
            Self::Recognize(response) => Some(&response.id),
            Self::Error(response) => response.id.as_deref(),
        }
    }
}
