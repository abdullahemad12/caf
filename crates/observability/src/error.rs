use std::{error::Error, fmt, io, str::FromStr, sync::Arc};

use strum::{Display, EnumString};
use tonic::{
    Code, Status,
    metadata::{Ascii, MetadataKey, MetadataMap, MetadataValue},
};
// TODO: Add a utility to print an error, print a debug statement with debug formatting (which
// should endup to the debug log file) and exit the program with an error

pub const CAF_ERROR_CODE_HEADER: &str = "caf-error-code";

#[derive(Debug, PartialEq, EnumString, Display, Clone)]
pub enum CafErrorCode {
    Internal,
    NetworkFailure,
    IoOperationCanceled,
    OutOfDiskStorage,
    PackageNotFound,
    PackageVersionHasNoProviders,
    FileNotFound,
    PackageNotInstalled,
    FileAlreadyExists,
    AnotherInstanceRunning,
    OsPermissionDenied,
    CliPermissionDenied,
    InvalidConfig,
    BrokenDatabaseState,
    Todo,
}

impl CafErrorCode {
    pub fn to_status_error_code(&self) -> Code {
        match self {
            Self::Internal => Code::Internal,
            Self::PackageNotFound | Self::FileNotFound | Self::PackageVersionHasNoProviders => {
                Code::NotFound
            }
            Self::PackageNotInstalled => Code::FailedPrecondition,
            Self::FileAlreadyExists | Self::AnotherInstanceRunning => Code::AlreadyExists,
            Self::OsPermissionDenied | Self::CliPermissionDenied => Code::PermissionDenied,
            Self::IoOperationCanceled => Code::Cancelled,
            Self::OutOfDiskStorage => Code::ResourceExhausted,
            Self::NetworkFailure => Code::Unavailable,
            Self::InvalidConfig => Code::InvalidArgument,
            Self::BrokenDatabaseState => Code::FailedPrecondition,
            Self::Todo => Code::Unimplemented,
        }
    }

    fn from(err: &(dyn std::error::Error + Send + 'static)) -> Self {
        if let Some(stat_err) = err.downcast_ref::<Status>() {
            return stat_err.metadata().get(CAF_ERROR_CODE_HEADER).map_or(
                Self::Internal,
                |ascii_val| {
                    ascii_val.to_str().map_or(Self::Internal, |raw_code| {
                        CafErrorCode::from_str(raw_code).unwrap_or(Self::Internal)
                    })
                },
            );
        }

        if let Some(io_err) = err.downcast_ref::<io::Error>() {
            return match io_err.kind() {
                io::ErrorKind::NotFound => Self::FileNotFound,
                io::ErrorKind::Interrupted => Self::IoOperationCanceled,
                io::ErrorKind::StorageFull => Self::OutOfDiskStorage,
                io::ErrorKind::PermissionDenied => Self::OsPermissionDenied,
                io::ErrorKind::AlreadyExists => Self::FileAlreadyExists,
                _ => Self::Internal, // support more as needed
            };
        }

        Self::Internal
    }
}

pub struct CafError {
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
    code: CafErrorCode,
}

impl CafError {
    pub fn new(code: CafErrorCode, message: impl Into<String>) -> Self {
        CafError {
            message: message.into(),
            source: None,
            code,
        }
    }

    pub fn to_status_error(self) -> Status {
        let metadata = {
            let mut metadata = MetadataMap::new();
            [(
                MetadataKey::<Ascii>::from_static(CAF_ERROR_CODE_HEADER),
                &self.code,
            )]
            .iter()
            .filter_map(|(k, v)| {
                let parsed_val = v.to_string().parse::<MetadataValue<_>>().ok()?;
                Some((k, parsed_val.clone()))
            })
            .for_each(|(k, v)| {
                metadata.insert(k, v);
            });
            metadata
        };

        Status::with_metadata(
            self.code.to_status_error_code(),
            self.to_string().clone(),
            metadata,
        )
        .set_source(Arc::new(self))
        .clone()
    }
}

impl fmt::Display for CafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source_message: String = match self.code {
            CafErrorCode::Internal => self.source.as_ref().map_or(String::new(), |it| match it
                .downcast_ref::<Status>()
            {
                Some(stat_err) => stat_err.message().to_string(),
                None => it.to_string(),
            }),
            CafErrorCode::BrokenDatabaseState => "the database is in a broken state".to_string(),
            CafErrorCode::AnotherInstanceRunning => {
                "another instance of the caf daemon is running".to_string()
            }
            CafErrorCode::OsPermissionDenied => {
                "the caf daemon does not have the required permissions to perform this action"
                    .to_string()
            }
            CafErrorCode::OutOfDiskStorage => {
                "the disk where caf stores its data is full".to_string()
            }
            CafErrorCode::CliPermissionDenied => {
                "your user does not have the permission to execute this command".to_string()
            }
            CafErrorCode::PackageNotInstalled => "the package is not installed".to_string(),
            CafErrorCode::PackageNotFound => "the package is not found".to_string(),
            CafErrorCode::PackageVersionHasNoProviders => {
                "the package or its specific version has no providers".to_string()
            }
            CafErrorCode::FileAlreadyExists => "the file already exists".to_string(),
            CafErrorCode::IoOperationCanceled => "the operation was canceled".to_string(),
            CafErrorCode::InvalidConfig => "invalid Caf configuration file".to_string(),
            CafErrorCode::NetworkFailure => "network failure".to_string(),
            CafErrorCode::FileNotFound => "the file is not found".to_string(),
            CafErrorCode::Todo => "this functionality is not yet implemented".to_string(),
        };

        write!(f, "{}: {}", self.message, source_message)
    }
}

impl fmt::Debug for CafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source.as_ref() {
            Some(source) => write!(f, "{}: {:?}", self.message, source),
            None => write!(f, "{}", self.message),
        }
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
    fn wrap_err_with_code(self, code: CafErrorCode, msg: impl Into<String>) -> Result<T, CafError>;
}

impl<T, E> WrapErrorInResult<T> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError> {
        self.map_err(|err| CafError {
            code: CafErrorCode::from(&err),
            message: msg.into(),
            source: Some(Box::new(err)),
        })
    }

    fn wrap_err_with_code(self, code: CafErrorCode, msg: impl Into<String>) -> Result<T, CafError> {
        self.map_err(|err| CafError {
            message: msg.into(),
            source: Some(Box::new(err)),
            code,
        })
    }
}

// type extension for Error for a cleaner usage
pub trait WrapError<T> {
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError>;
    fn wrap_err_with_code(self, code: CafErrorCode, msg: impl Into<String>) -> Result<T, CafError>;
}

impl<E, T> WrapError<T> for E
where
    E: Error + Send + Sync + 'static,
{
    fn wrap_err(self, msg: impl Into<String>) -> Result<T, CafError> {
        Err(CafError {
            code: CafErrorCode::from(&self),
            message: msg.into(),
            source: Some(Box::new(self)),
        })
    }

    fn wrap_err_with_code(self, code: CafErrorCode, msg: impl Into<String>) -> Result<T, CafError> {
        Err(CafError {
            message: msg.into(),
            source: Some(Box::new(self)),
            code,
        })
    }
}

pub trait MapErrToStatus<T> {
    fn map_err_to_status(self) -> Result<T, Status>;
}

impl<T> MapErrToStatus<T> for Result<T, CafError> {
    fn map_err_to_status(self) -> Result<T, Status> {
        self.map_err(|it| it.to_status_error())
    }
}

pub trait LoggedUnwrap<T> {
    fn logged_unwrap(self) -> T;
}

impl<T> LoggedUnwrap<T> for Result<T, CafError> {
    fn logged_unwrap(self) -> T {
        self.unwrap_or_else(|it| {
            tracing::error!("{}", it);
            tracing::debug!("{:?}", it);
            std::process::exit(1)
        })
    }
}
