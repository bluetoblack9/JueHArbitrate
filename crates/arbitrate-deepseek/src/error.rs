use std::fmt;

use serde::Deserialize;

use crate::wire::EncodeError;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub kind: String,
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub(crate) fn from_body(status: u16, body: &[u8]) -> Self {
        #[derive(Deserialize)]
        struct Envelope {
            error: Payload,
        }

        #[derive(Deserialize)]
        struct Payload {
            #[serde(rename = "type")]
            kind: Option<String>,
            code: Option<String>,
            message: Option<String>,
        }

        if let Ok(Envelope { error }) = serde_json::from_slice(body)
            && let Some(message) = error.message.filter(|message| !message.is_empty())
        {
            return Self {
                status,
                kind: error.kind.unwrap_or_default(),
                code: error.code.unwrap_or_default(),
                message,
            };
        }

        Self {
            status,
            kind: String::new(),
            code: String::new(),
            message: format!("status {status}: {}", String::from_utf8_lossy(body)),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("deepseek api error: ")?;
        if !self.kind.is_empty() {
            write!(f, "{}: ", self.kind)?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

#[derive(Debug)]
pub enum Error {
    MissingApiKey,
    RequestConfig(String),
    Encode(EncodeError),
    Transport(reqwest::Error),
    Api(ApiError),
    Decode(serde_json::Error),
    Cancelled,
    Exhausted { attempts: u8, source: Box<Error> },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("deepseek: missing api key (set DEEPSEEK_API_KEY)"),
            Self::RequestConfig(message) => write!(f, "deepseek: request configuration: {message}"),
            Self::Encode(error) => write!(f, "deepseek: encode request: {error}"),
            Self::Transport(error) => write!(f, "deepseek: HTTP transport: {error}"),
            Self::Api(error) => error.fmt(f),
            Self::Decode(error) => write!(f, "deepseek: decode response: {error}"),
            Self::Cancelled => f.write_str("deepseek: request cancelled"),
            Self::Exhausted { attempts, source } => {
                write!(f, "{source} (after {attempts} attempts)")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::Api(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Exhausted { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
