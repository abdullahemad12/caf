use std::error;

pub fn log_error(err: impl error::Error) -> impl error::Error {
    eprintln!("{:?}", err);
    err
}
