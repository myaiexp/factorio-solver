use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlueprintError {
    #[error("missing version byte: blueprint string is empty")]
    MissingVersionByte,

    #[error("unsupported version byte: '{0}'")]
    UnsupportedVersion(char),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("zlib decompression error: {0}")]
    Zlib(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid data: {0}")]
    InvalidData(String),
}
