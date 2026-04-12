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
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum TagValue {
    #[default]
    /// No value
    None,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// String value
    String(
        #[cfg_attr(feature = "wasm", tsify(type = "String"))]
        #[cfg_attr(feature = "bincode", bincode(with_serde))]
        SharedString,
    ),
}
impl TagValue {
    /// Returns the type of the tag value.
    #[must_use]
    pub fn tpe(&self) -> TagType {
        match self {
            Self::None => TagType::None,
            Self::Bool(_) => TagType::Bool,
            Self::Int(_) => TagType::Int,
            Self::Float(_) => TagType::Float,
            Self::String(_) => TagType::String,
        }
    }
}

impl fmt::Debug for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
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
        }
    }
}

impl Ord for TagValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            // First the easy cases, if we have two values of the same type,
            // compare them directly
            (TagValue::None, TagValue::None) => std::cmp::Ordering::Equal,
            (TagValue::Int(a), TagValue::Int(b)) => a.cmp(b),
            (TagValue::Float(a), TagValue::Float(b)) => OrderedFloat(*a).cmp(&OrderedFloat(*b)),
            (TagValue::String(a), TagValue::String(b)) => a.cmp(b),
            (TagValue::Bool(a), TagValue::Bool(b)) => a.cmp(b),

            // If we have two numeric values of different types,
            // cast them to f64 for and compare
            (TagValue::Int(i), TagValue::Float(f)) =>
            {
                #[allow(clippy::cast_precision_loss)]
                OrderedFloat(*i as f64).cmp(&OrderedFloat(*f))
            }
            (TagValue::Float(f), TagValue::Int(i)) =>
            {
                #[allow(clippy::cast_precision_loss)]
                OrderedFloat(*f).cmp(&OrderedFloat(*i as f64))
            }

            // This are now in reverse order of precedence
            // the rule we use is 'the more complex the type is the
            // greater the ordering'

            // Everything greater than None
            (TagValue::None, _) => std::cmp::Ordering::Less,
            (_, TagValue::None) => std::cmp::Ordering::Greater,

            // The rest if larger than bool
            (TagValue::Bool(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Bool(_)) => std::cmp::Ordering::Greater,

            // now everything else is larger than int
            (TagValue::Int(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Int(_)) => std::cmp::Ordering::Greater,

            // now everything else is larger than float
            (TagValue::Float(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Float(_)) => std::cmp::Ordering::Greater,
            // string is the largest type - this is a unreachable case
            // as the prior matches already handle this.
            // (TagValue::String(_), _) => std::cmp::Ordering::Less,
            // (_, TagValue::String(_)) => std::cmp::Ordering::Greater,
        }
    }
}
impl PartialOrd for TagValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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

    /// Returns the length of the tag value
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            TagValue::None => 0,
            TagValue::String(s) => s.len(),
            TagValue::Int(_) | TagValue::Float(_) => 8, // size of i64 or f64
            TagValue::Bool(_) => 1,                     // size of bool
        }
    }
    /// Returns true if the tag value is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            TagValue::None => true,
            TagValue::String(s) => s.is_empty(),
            TagValue::Bool(_) | TagValue::Int(_) | TagValue::Float(_) => false, // bool, i64 and f64 are never empty
        }
    }
}

impl Hash for TagValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            TagValue::None => (),
            TagValue::String(s) => s.hash(state),
            TagValue::Int(i) => i.hash(state),
            TagValue::Float(fl) => OrderedFloat(*fl).hash(state),
            TagValue::Bool(b) => b.hash(state),
        }
    }
}

// FIXME! This is not good since we have floats
impl Eq for TagValue {}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagValue::None => write!(f, "None"),
            TagValue::String(s) => {
                let mut hasher = DefaultHasher::new();
                s.hash(&mut hasher);

                write!(f, "\"<PII Safe String: {}>\"", &hasher.finish())
            }
            TagValue::Int(i) => write!(f, "{i}"),
            TagValue::Float(fl) => write!(f, "{fl}"),
            TagValue::Bool(b) => write!(f, "{b}"),
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
