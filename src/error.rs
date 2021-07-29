use thiserror::Error;

use redis::RedisError;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("error parsing string from redis result: {0}")]
    RedisTypeError(redis::RedisError),
    #[error("error executing redis command: {0}")]
    RedisCMDError(redis::RedisError),
    #[error("error creating Redis client: {0}")]
    RedisClientError(redis::RedisError),
    #[error("failed to download the task, internal error {0}")]
    DownloadTaskError(String),
}

impl warp::reject::Reject for Error {}

impl std::convert::From<RedisError> for Error {
    fn from(e: RedisError) -> Error {
        return Error::RedisTypeError(e);
    }
}
