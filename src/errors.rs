use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransformError {
    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),
    #[error("Failed to parse JavaScript: {message}")]
    JsParse { message: String },
    #[error("Failed to parse CSS: {message}")]
    CssParse { message: String },
    #[error("JavaScript parsing panicked")]
    JsPanicParse,
    #[error("Failed to transform CSS: {message}")]
    CssTransform { message: String },
    #[error("URL '{url}' not found in replacement map")]
    UrlNotFound { url: String },
    #[error("Failed to serialize CSS: {message}")]
    CssSerialize { message: String },
}

#[derive(Error, Debug)]
pub enum DependencyError {
    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),
    #[error("Failed to parse JavaScript dependencies: {message}")]
    JsParse { message: String },
    #[error("Failed to parse CSS dependencies: {message}")]
    CssParse { message: String },
    #[error("JavaScript parsing panicked")]
    JsPanicParse,
    #[error("Failed to extract dependencies: {message}")]
    Extract { message: String },
}

pub type TransformResult<T> = std::result::Result<T, TransformError>;
pub type DependencyResult<T> = std::result::Result<T, DependencyError>;
