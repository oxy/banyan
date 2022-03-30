use std::io;

pub enum Error {
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::Io(ref err) => match err.raw_os_error() {
                Some(e) => Error::Io(io::Error::from_raw_os_error(e)),
                None => Error::Io(io::Error::new(err.kind(), err.to_string())),
            },
        }
    }
}
