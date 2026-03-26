mod error;
mod reader;

pub use error::ReaderError;

pub use reader::{FwrFieldIter, Reader, Record, RecordIter};
