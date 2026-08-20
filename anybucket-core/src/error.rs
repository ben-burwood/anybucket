use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no active connection is selected")]
    NoActiveConnection,

    #[error("connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("missing credentials for connection: {0}")]
    MissingCredentials(String),

    #[error("secret store error: {0}")]
    Secret(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("S3 error: {0}")]
    S3(String),

    // Reserved for provider-capability degradation (see plan: future metrics /
    // graceful handling of endpoints that lack an API).
    #[allow(dead_code)]
    #[error("this provider does not support {0}")]
    Unsupported(&'static str),

    #[error("download failed: {0}")]
    Download(String),

    #[error("the active connection is read-only")]
    ReadOnly,

    #[error("upload failed: {0}")]
    Upload(String),

    #[error("the active connection does not permit deletes")]
    DeleteNotAllowed,

    #[error("the active connection does not permit bucket administration")]
    AdminNotAllowed,

    #[error("a bucket named '{0}' already exists")]
    BucketAlreadyExists(String),

    #[error("bucket '{0}' is not empty")]
    BucketNotEmpty(String),

    #[error("delete failed: {0}")]
    Delete(String),

    #[error("copy failed: {0}")]
    Copy(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::NoActiveConnection => "no_active_connection",
            AppError::ConnectionNotFound(_) => "connection_not_found",
            AppError::MissingCredentials(_) => "missing_credentials",
            AppError::Secret(_) => "secret",
            AppError::Config(_) => "config",
            AppError::S3(_) => "s3",
            AppError::Unsupported(_) => "unsupported",
            AppError::Download(_) => "download",
            AppError::ReadOnly => "read_only",
            AppError::Upload(_) => "upload",
            AppError::DeleteNotAllowed => "delete_not_allowed",
            AppError::AdminNotAllowed => "admin_not_allowed",
            AppError::BucketAlreadyExists(_) => "bucket_already_exists",
            AppError::BucketNotEmpty(_) => "bucket_not_empty",
            AppError::Delete(_) => "delete",
            AppError::Copy(_) => "copy",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

// Convenience conversion for the AWS SDK's `SdkError<E, R>`.
impl<E, R> From<aws_smithy_runtime_api::client::result::SdkError<E, R>> for AppError
where
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug + Send + Sync + 'static,
{
    fn from(err: aws_smithy_runtime_api::client::result::SdkError<E, R>) -> Self {
        AppError::S3(aws_smithy_types::error::display::DisplayErrorContext(&err).to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Config(err.to_string())
    }
}
