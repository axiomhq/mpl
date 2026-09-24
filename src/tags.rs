//! Series tags and tag values
use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
};

use ordered_float::OrderedFloat;
use strumbra::SharedString;

use crate::{query::TagType, types::StrumbraError};

/// Value for a tag k/v pair
#[derive(Clone, PartialEq, serde::Deserialize, serde::Serialize, Default)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
#[serde(untagged)]
pub enum TagValue {
    #[default]
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// String value
    String(#[cfg_attr(feature = "bincode", bincode(with_serde))] SharedString),
    /// Array value
    Array(Vec<TagValue>),
}
impl TagValue {
    /// Returns the type of the tag value.
    #[must_use]
    pub fn tpe(&self) -> TagType {
        match self {
            Self::Null => TagType::Null,
            Self::Bool(_) => TagType::Bool,
            Self::Int(_) => TagType::Int,
            Self::Float(_) => TagType::Float,
            Self::String(_) => TagType::String,
            Self::Array(_) => TagType::Array,
        }
    }
}

impl fmt::Debug for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Bool(arg0) => f.debug_tuple("Bool").field(arg0).finish(),
            Self::Int(arg0) => f.debug_tuple("Int").field(arg0).finish(),
            Self::Float(arg0) => f.debug_tuple("Float").field(arg0).finish(),
            Self::String(arg0) => {
                // Since arguments could include PII we do replace them with a hash
                let mut hasher = DefaultHasher::new();
                arg0.hash(&mut hasher);
                f.debug_tuple("PiiSafeString")
                    .field(&hasher.finish())
                    .finish()
            }
            Self::Array(arg0) => f.debug_tuple("Array").field(arg0).finish(),
        }
    }
}

impl TagValue {
    /// Tries to access the tag value as a string
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        if let TagValue::String(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    /// Returns the length of the tag value in estimated bytes
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            TagValue::Null => 0,
            TagValue::String(s) => s.len(),
            TagValue::Array(a) => a.iter().map(TagValue::len).sum(),
            TagValue::Int(_) | TagValue::Float(_) => 8, // size of i64 or f64
            TagValue::Bool(_) => 1,                     // size of bool
        }
    }
    /// Returns true if the tag value is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            TagValue::Null => true,
            TagValue::String(s) => s.is_empty(),
            TagValue::Array(a) => a.is_empty(),
            TagValue::Bool(_) | TagValue::Int(_) | TagValue::Float(_) => false, // bool, i64 and f64 are never empty
        }
    }
}

impl Hash for TagValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            TagValue::Null => (),
            TagValue::String(s) => s.hash(state),
            TagValue::Int(i) => i.hash(state),
            TagValue::Float(fl) => OrderedFloat(*fl).hash(state),
            TagValue::Bool(b) => b.hash(state),
            TagValue::Array(a) => a.hash(state),
        }
    }
}

// FIXME! This is not good since we have floats
impl Eq for TagValue {}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagValue::Null => write!(f, "Null"),
            TagValue::String(s) => {
                let mut hasher = DefaultHasher::new();
                s.hash(&mut hasher);

                write!(f, "\"<PII Safe String: {}>\"", hasher.finish())
            }
            TagValue::Int(i) => write!(f, "{i}"),
            TagValue::Float(fl) => write!(f, "{fl}"),
            TagValue::Bool(b) => write!(f, "{b}"),
            TagValue::Array(a) => {
                write!(f, "[")?;
                for (i, item) in a.iter().enumerate() {
                    item.fmt(f)?;
                    if i < a.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
        }
    }
}

impl From<i64> for TagValue {
    fn from(i: i64) -> Self {
        TagValue::Int(i)
    }
}

impl From<f64> for TagValue {
    fn from(f: f64) -> Self {
        TagValue::Float(f)
    }
}

impl From<bool> for TagValue {
    fn from(b: bool) -> Self {
        TagValue::Bool(b)
    }
}
impl TryFrom<String> for TagValue {
    type Error = StrumbraError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Ok(TagValue::String(SharedString::try_from(s)?))
    }
}
impl TryFrom<&str> for TagValue {
    type Error = StrumbraError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(TagValue::String(SharedString::try_from(s)?))
    }
}
