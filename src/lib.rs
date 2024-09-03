mod error;
mod reader;

pub use error::ReaderError;

pub use reader::{FwfFileReader, FwfRecord, FwfRecordIter, FwrFieldIter};
