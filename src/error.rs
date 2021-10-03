use config::ConfigError;
use redis::RedisError;
use rusoto_core::RusotoError;
use rusoto_s3::{CreateBucketError, DeleteObjectError, GetObjectError};
use std::convert::From;
use thiserror::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum Error {
    #[error("error parsing string from redis result: {0}")]
    RedisTypeError(redis::RedisError),
    #[error("error executing redis command: {0}")]
    RedisCMDError(redis::RedisError),
    #[error("error creating Redis client: {0}")]
    RedisClientError(redis::RedisError),
    #[error("sled error: {0}")]
    SledError(sled::Error),
    #[error("sled transaction error: {0}")]
    SledUnabortableTransactionError(sled::transaction::UnabortableTransactionError),
    #[error("outbound request failed: {0}")]
    RequestError(reqwest::Error),
    #[error("upstream request is not successful: {0:?}")]
    UpstreamRequestError(reqwest::Response),
    #[error("{0}")]
    ConfigDeserializeError(config::ConfigError),
    #[error("invalid configuration: {0}")]
    ConfigInvalid(String),
    #[error("{0}")]
    IoError(std::io::Error),
    #[error("{0}")]
    OtherError(String),
    #[error("failed to get rusoto object: {0}")]
    RusotoGetObjectError(RusotoError<GetObjectError>),
    #[error("failed to delete rusoto object: {0}")]
    RusotoDeleteObjectError(RusotoError<DeleteObjectError>),
    #[error("faield to crate bucket: {0}")]
    RusotoCreateBucketError(RusotoError<CreateBucketError>),
}

impl warp::reject::Reject for Error {}

impl From<RedisError> for Error {
    fn from(e: RedisError) -> Error {
        Error::RedisTypeError(e)
    }
}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Error {
        Error::ConfigDeserializeError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IoError(e)
    }
}

impl From<sled::transaction::UnabortableTransactionError> for Error {
    fn from(e: sled::transaction::UnabortableTransactionError) -> Error {
        Error::SledUnabortableTransactionError(e)
    }
}

impl From<RusotoError<CreateBucketError>> for Error {
    fn from(e: RusotoError<CreateBucketError>) -> Error {
        Error::RusotoCreateBucketError(e)
    }
}

impl From<RusotoError<GetObjectError>> for Error {
    fn from(e: RusotoError<GetObjectError>) -> Error {
        Error::RusotoGetObjectError(e)
    }
}

impl From<RusotoError<DeleteObjectError>> for Error {
    fn from(e: RusotoError<DeleteObjectError>) -> Error {
        Error::RusotoDeleteObjectError(e)
    }
}
