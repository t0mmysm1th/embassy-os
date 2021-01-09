use std::fmt::Display;

use hyper::{Body, Response, StatusCode};
use serde::{Deserialize, Serialize};

pub const GENERAL_ERROR: i32 = 1;
pub const FILESYSTEM_ERROR: i32 = 2;
pub const DOCKER_ERROR: i32 = 3;
pub const CFG_SPEC_VIOLATION: i32 = 4;
pub const CFG_RULES_VIOLATION: i32 = 5;
pub const NOT_FOUND: i32 = 6;
pub const INVALID_BACKUP_PASSWORD: i32 = 7;
pub const VERSION_INCOMPATIBLE: i32 = 8;
pub const NETWORK_ERROR: i32 = 9;
pub const REGISTRY_ERROR: i32 = 10;
pub const SERDE_ERROR: i32 = 11;
pub const UNRECOGNIZED_COMMAND: i32 = 12;

fn code_to_status(code: i32) -> StatusCode {
    match code {
        CFG_SPEC_VIOLATION => StatusCode::FORBIDDEN,
        CFG_RULES_VIOLATION => StatusCode::FORBIDDEN,
        NOT_FOUND => StatusCode::NOT_FOUND,
        INVALID_BACKUP_PASSWORD => StatusCode::FORBIDDEN,
        VERSION_INCOMPATIBLE => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Debug, Error, Deserialize, Serialize)]
#[error("{message}")]
pub struct Error {
    #[serde(with = "serde_anyhow")]
    pub message: anyhow::Error,
    pub code: i32,
}
impl Error {
    pub fn new<E: Into<anyhow::Error>>(e: E, code: i32) -> Self {
        Error {
            message: e.into(),
            code,
        }
    }
    pub fn to_response(&self, allow_cbor: bool) -> Response<Body> {
        if allow_cbor {
            let body = serde_cbor::to_vec(self).unwrap();
            Response::builder()
                .header("content-type", "application/cbor")
                .header("content-length", body.len())
                .status(code_to_status(self.code))
                .body(body.into())
                .unwrap()
        } else {
            let body = serde_json::to_vec(self).unwrap();
            Response::builder()
                .header("content-type", "application/json")
                .header("content-length", body.len())
                .status(code_to_status(self.code))
                .body(body.into())
                .unwrap()
        }
    }
}
impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error {
            message: e,
            code: GENERAL_ERROR,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error {
            message: e.into(),
            code: FILESYSTEM_ERROR,
        }
    }
}
pub trait ResultExt<T, E>
where
    Self: Sized,
{
    fn with_code(self, code: i32) -> Result<T, Error>;
    fn with_ctx<F: FnOnce(&E) -> (i32, D), D: Display + Send + Sync + 'static>(
        self,
        f: F,
    ) -> Result<T, Error>;
    fn no_code(self) -> Result<T, Error>;
}
impl<T, E> ResultExt<T, E> for Result<T, E>
where
    anyhow::Error: From<E>,
{
    fn with_code(self, code: i32) -> Result<T, Error> {
        #[cfg(not(feature = "production"))]
        assert!(code != 0);
        self.map_err(|e| Error {
            message: e.into(),
            code,
        })
    }

    fn with_ctx<F: FnOnce(&E) -> (i32, D), D: Display + Send + Sync + 'static>(
        self,
        f: F,
    ) -> Result<T, Error> {
        self.map_err(|e| {
            let (code, ctx) = f(&e);
            let message = anyhow::Error::from(e).context(ctx);
            Error {
                code,
                message: message.into(),
            }
        })
    }

    fn no_code(self) -> Result<T, Error> {
        self.map_err(|e| Error {
            message: e.into(),
            code: GENERAL_ERROR,
        })
    }
}

#[macro_export]
macro_rules! ensure_code {
    ($x:expr, $c:expr, $fmt:expr $(, $arg:expr)*) => {
        if !($x) {
            return Err(crate::Error {
                message: anyhow!($fmt, $($arg, )*),
                code: $c,
            });
        }
    };
}

mod serde_anyhow {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Debug, Error)]
    #[error("{0}")]
    pub struct DeserializedError(String);

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<anyhow::Error, D::Error> {
        Ok(DeserializedError(String::deserialize(deserializer)?).into())
    }

    pub fn serialize<S: Serializer>(e: &anyhow::Error, serializer: S) -> Result<S::Ok, S::Error> {
        format!("{}", e).serialize(serializer)
    }
}
