use zlim_core::error::Error;

// -----------------------------------------------------------------------------
// MetaParseError

/// An error that occurs while deserializing an asset's `.meta` data.
#[derive(Error, Debug, Clone)]
#[repr(transparent)]
#[error("failed to deserialize asset meta: `{_0}`")]
pub struct MetaParseError(ron::de::SpannedError);

impl From<ron::de::SpannedError> for MetaParseError {
    #[cold]
    fn from(value: ron::de::SpannedError) -> Self {
        Self(value)
    }
}

// -----------------------------------------------------------------------------
// AssetMetaParseError

/// The `.meta` of the asset at `path` could not be deserialized.
#[derive(Error, Debug, Clone)]
#[error("failed to deserialize asset meta for `{path}`: {}", error.0)]
pub struct AssetMetaParseError {
    /// The path of the asset whose `.meta` could not be deserialized.
    pub path: Box<str>, // reduce the struct size of errors.
    /// The deserialization error itself.
    pub error: MetaParseError,
}

// -----------------------------------------------------------------------------
