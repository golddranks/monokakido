use std::{io::Error as IoError, str::Utf8Error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Transmute,
    Validate,
    FopenError,
    FstatError,
    MmapError,
    ZlibError,
    Utf8Error,
    RecordTooLarge,
    IncorrectStreamLength,
    BufferTooSmall,
    IndexMismach,
    NotFound,
    NoDictJsonFound,
    InvalidDictJson,
    IOError,
    NoContentFilesFound,
    InvalidIndex,
    InvalidAudioFormat,
}

impl From<IoError> for Error {
    fn from(_: IoError) -> Self {
        Error::IOError
    }
}

impl From<Utf8Error> for Error {
    fn from(_: Utf8Error) -> Self {
        Error::Utf8Error
    }
}
