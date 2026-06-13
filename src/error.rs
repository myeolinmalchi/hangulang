use thiserror::Error;

/// Errors produced by the HWP → DocLang conversion pipeline.
#[derive(Debug, Error)]
pub enum ConvertError {
    /// rhwp failed to parse the input bytes.
    #[error("failed to parse HWP document: {0}")]
    Parse(String),

    /// Encrypted documents are rejected by rhwp itself (ParseError::EncryptedDocument).
    #[error("encrypted HWP documents are not supported")]
    EncryptedDocument,

    /// Distribution (배포용) documents parse successfully in rhwp, but converting them
    /// is out of scope for v1 by policy — rejected explicitly at the adapter boundary.
    #[error("distribution (배포용) HWP documents are not supported in v1")]
    DistributionDocumentUnsupported,

    /// HWP 3.x and legacy HWPML inputs are out of scope for v1.
    #[error("unsupported input format: {0}")]
    UnsupportedFormat(&'static str),

    /// XML serialization failure (quick-xml).
    #[error("failed to serialize DocLang XML: {0}")]
    Xml(String),
}
