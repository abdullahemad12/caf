use std::{error::Error, fmt};

#[derive(Debug)]
pub struct CafError {
    message: String,
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl fmt::Display for CafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for CafError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

// type extension for Result<T, CafError> for a cleaner usage
pub trait WrapError<T, E> {
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError>;
}

impl<T, E> WrapError<T, E> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError> {
        self.map_err(|err| CafError {
            message: msg.into(),
            source: Box::new(err),
        })
    }
}
