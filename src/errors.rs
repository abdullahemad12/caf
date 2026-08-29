use std::{error::Error, fmt};

#[derive(Debug)]
pub struct CafError {
    message: String,
    source: Option<Box<dyn Error + Send + 'static>>,
}

impl CafError {
    pub fn new(message: impl Into<String>) -> Self {
        CafError {
            message: message.into(),
            source: None,
        }
    }
}

impl fmt::Display for CafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source_message = self
            .source
            .as_ref()
            .map_or(String::new(), |it| it.to_string());

        write!(f, "{}\n{}", self.message, source_message)
    }
}

impl Error for CafError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.source.as_ref() {
            Some(err) => Some(err.as_ref()),
            None => None,
        }
    }
}

// type extension for Result<T, CafError> for a cleaner usage
pub trait WrapErrorInResult<T> {
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError>;
}

impl<T, E> WrapErrorInResult<T> for Result<T, E>
where
    E: Error + Send + 'static,
{
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError> {
        self.map_err(|err| CafError {
            message: msg.into(),
            source: Some(Box::new(err)),
        })
    }
}

// type extension for Error for a cleaner usage
pub trait WrapError<T> {
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError>;
}

impl<E, T> WrapError<T> for E
where
    E: Error + Send + 'static,
{
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError> {
        Err(CafError {
            message: msg.into(),
            source: Some(Box::new(self)),
        })
    }
}
