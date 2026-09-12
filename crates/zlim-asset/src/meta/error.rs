use core::error::Error;
use ron::de::SpannedError;
use zlim_core::error::Error;

/// An error that occurs while deserializing `AssetMeta`.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DeserializeMetaError {
    #[error("Failed to deserialize asset meta: {_0}")]
    Normal(String),
    #[error("Failed to deserialize minimal asset config: {_0}")]
    AssetConfig(String),
    #[error("Failed to deserialize minimal process info: {_0}")]
    ProcessInfo(String),
}

impl From<SpannedError> for DeserializeMetaError {
    #[inline]
    fn from(value: SpannedError) -> Self {
        Self::Normal(value.to_string())
    }
}

impl DeserializeMetaError {
    /// Create a [`DeserializeMetaError::Normal`] from given error.
    #[cold]
    pub fn normal(err: impl Error) -> Self {
        Self::Normal(err.to_string())
    }

    /// Create a [`DeserializeMetaError::AssetConfig`] from given error.
    #[cold]
    pub fn asset_config(err: impl Error) -> Self {
        Self::AssetConfig(err.to_string())
    }

    /// Create a [`DeserializeMetaError::ProcessInfo`] from given error.
    #[cold]
    pub fn process_info(err: impl Error) -> Self {
        Self::ProcessInfo(err.to_string())
    }
}
