use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("Io error: {0}")]
    Io(std::io::Error),

    #[error("Empty line")]
    EmptyLine,

    #[error("Width Missmatch")]
    WidthMismatch(usize, usize),
}

impl From<std::io::Error> for ReaderError {
    fn from(value: std::io::Error) -> Self {
        ReaderError::Io(value)
    }
}
