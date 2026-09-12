use serde::{Deserialize, Serialize, de::Visitor};

// -----------------------------------------------------------------------------
// FormatVersion

/// The version of the metadata format being used.
#[derive(Debug, Default, Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum FormatVersion {
    #[default]
    V1_0,
}

// -----------------------------------------------------------------------------
// FormatVersionMinimal

/// A minimal counterpart to [`FormatVersion`] that exists to speed up
/// deserialization in cases where the whole `AssetMeta` isn't necessary.
#[derive(Deserialize)]
pub struct FormatVersionMinimal {
    #[serde(default)]
    pub format_version: FormatVersion,
}

// -----------------------------------------------------------------------------
// Serialize & Deserialize

impl Serialize for FormatVersion {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            FormatVersion::V1_0 => serializer.serialize_str("1.0"),
        }
    }
}

impl<'de> Deserialize<'de> for FormatVersion {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(VersionVisitor)
    }
}

struct VersionVisitor;

impl Visitor<'_> for VersionVisitor {
    type Value = FormatVersion;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(formatter, "version string in {:?}", ["1.0"])
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "1.0" => Ok(FormatVersion::V1_0),
            _ => {
                let unexp = serde::de::Unexpected::Str(v);
                Err(serde::de::Error::invalid_value(unexp, &self))
            }
        }
    }
}

// -----------------------------------------------------------------------------
