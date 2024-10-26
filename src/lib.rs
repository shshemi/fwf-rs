mod error;
mod reader;

pub use error::ReaderError;

pub use reader::{Record, RecordIter, FwrFieldIter, Reader};
