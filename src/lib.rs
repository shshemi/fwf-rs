mod error;
mod reader;

pub use error::ReaderError;

pub use reader::{FwfFieldIter, Reader, Record, RecordIter};
