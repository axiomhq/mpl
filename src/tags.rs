//! Series tags and tag values
#[cfg(feature = "bincode")]
use bincode::error::{AllowedEnumVariants, DecodeError};
use ordered_float::OrderedFloat;
use std::{
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
};
use strumbra::SharedString;

use crate::{query::TagType, types::StrumbraError};

#[cfg(feature = "bincode")]
use crate::types::StringDeduper;

/// Value for a tag k/v pair
#[derive(Clone, PartialEq, serde::Deserialize, serde::Serialize, Default)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode))]
#[cfg_attr(all(test, feature = "bincode"), derive(strum::EnumDiscriminants))]
#[cfg_attr(
    all(test, feature = "bincode"),
    strum_discriminants(derive(strum::VariantArray), vis(pub(crate)))
)]
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

#[cfg(feature = "bincode")]
impl<'de, Context: StringDeduper> bincode::BorrowDecode<'de, Context> for TagValue {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let idx = u32::borrow_decode(decoder)?;
        match idx {
            0 => Ok(TagValue::Null),
            1 => bool::borrow_decode(decoder).map(TagValue::Bool),
            2 => i64::borrow_decode(decoder).map(TagValue::Int),
            3 => f64::borrow_decode(decoder).map(TagValue::Float),
            4 => {
                let s = <&str>::borrow_decode(decoder)?;
                decoder
                    .context()
                    .string(s)
                    .map_err(|_| DecodeError::Other("failed to dedup string"))
                    .map(TagValue::String)
            }
            5 => Vec::<TagValue>::borrow_decode(decoder).map(Self::Array),
            found => Err(DecodeError::UnexpectedVariant {
                type_name: "TagValue",
                allowed: &AllowedEnumVariants::Range { min: 0, max: 5 },
                found,
            }),
        }
    }
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

impl Ord for TagValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            // First the easy cases, if we have two values of the same type,
            // compare them directly
            (TagValue::Null, TagValue::Null) => std::cmp::Ordering::Equal,
            (TagValue::Int(a), TagValue::Int(b)) => a.cmp(b),
            (TagValue::Float(a), TagValue::Float(b)) => OrderedFloat(*a).cmp(&OrderedFloat(*b)),
            (TagValue::String(a), TagValue::String(b)) => a.cmp(b),
            (TagValue::Bool(a), TagValue::Bool(b)) => a.cmp(b),
            (TagValue::Array(a), TagValue::Array(b)) => a.cmp(b),

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

            // Everything greater than Null
            (TagValue::Null, _) => std::cmp::Ordering::Less,
            (_, TagValue::Null) => std::cmp::Ordering::Greater,

            // The rest if larger than bool
            (TagValue::Bool(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Bool(_)) => std::cmp::Ordering::Greater,

            // now everything else is larger than int
            (TagValue::Int(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Int(_)) => std::cmp::Ordering::Greater,

            // now everything else is larger than float
            (TagValue::Float(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::Float(_)) => std::cmp::Ordering::Greater,
            // now everything else is larger than string
            (TagValue::String(_), _) => std::cmp::Ordering::Less,
            (_, TagValue::String(_)) => std::cmp::Ordering::Greater,
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

#[cfg(all(test, feature = "bincode"))]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use strum::VariantArray;

    /// Wire index of each variant; the exhaustive match pins the on-disk format.
    fn wire_index(value: &TagValue) -> u32 {
        match value {
            TagValue::Null => 0,
            TagValue::Bool(_) => 1,
            TagValue::Int(_) => 2,
            TagValue::Float(_) => 3,
            TagValue::String(_) => 4,
            TagValue::Array(_) => 5,
        }
    }

    /// Position of a variant in declaration order, which is the index the derived encoder writes.
    fn declaration_index(value: &TagValue) -> u32 {
        let discriminant = TagValueDiscriminants::from(value);
        let position = TagValueDiscriminants::VARIANTS
            .iter()
            .position(|v| *v == discriminant)
            .expect("discriminant is a variant");
        u32::try_from(position).expect("variant count fits")
    }

    fn last_wire_index() -> u32 {
        u32::try_from(TagValueDiscriminants::VARIANTS.len() - 1).expect("variant count fits")
    }

    /// Payload strategy for one variant; the exhaustive match ties it to the enum definition.
    fn variant_strategy(
        variant: TagValueDiscriminants,
        inner: BoxedStrategy<TagValue>,
    ) -> BoxedStrategy<TagValue> {
        use proptest::num::f64::{INFINITE, NEGATIVE, NORMAL, POSITIVE, SUBNORMAL, ZERO};
        match variant {
            TagValueDiscriminants::Null => Just(TagValue::Null).boxed(),
            TagValueDiscriminants::Bool => any::<bool>().prop_map(TagValue::Bool).boxed(),
            TagValueDiscriminants::Int => any::<i64>().prop_map(TagValue::Int).boxed(),
            TagValueDiscriminants::Float => {
                (POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO | INFINITE)
                    .prop_map(TagValue::Float)
                    .boxed()
            }
            TagValueDiscriminants::String => any::<String>()
                .prop_map(|s| TagValue::try_from(s).expect("string fits"))
                .boxed(),
            TagValueDiscriminants::Array => prop::collection::vec(inner, 0..8)
                .prop_map(TagValue::Array)
                .boxed(),
        }
    }

    /// Uniform choice over every variant, with `inner` as the array element strategy.
    fn any_variant(inner: &BoxedStrategy<TagValue>) -> BoxedStrategy<TagValue> {
        proptest::strategy::Union::new(
            TagValueDiscriminants::VARIANTS
                .iter()
                .map(|v| variant_strategy(*v, inner.clone())),
        )
        .boxed()
    }

    /// Generates every variant, nesting arrays up to four levels deep; the deepest arrays hold nulls.
    fn tag_value() -> impl Strategy<Value = TagValue> {
        any_variant(&Just(TagValue::Null).boxed())
            .prop_recursive(4, 32, 8, |inner| any_variant(&inner))
    }

    #[test]
    fn decoder_rejects_the_index_after_the_last_variant() {
        let config = bincode::config::standard();
        let unknown = last_wire_index() + 1;
        let bytes = bincode::encode_to_vec(unknown, config).expect("encodes");
        let result: Result<(TagValue, usize), DecodeError> =
            bincode::borrow_decode_from_slice_with_context(&bytes, config, ());
        match result {
            Err(DecodeError::UnexpectedVariant { allowed, found, .. }) => {
                assert_eq!(found, unknown);
                assert_eq!(
                    *allowed,
                    AllowedEnumVariants::Range {
                        min: 0,
                        max: last_wire_index()
                    }
                );
            }
            other => panic!("expected UnexpectedVariant, got {other:?}"),
        }
    }

    proptest! {
        /// The pinned wire table matches declaration order, so reordering the enum is caught.
        #[test]
        fn prop_wire_table_matches_declaration_order(value in tag_value()) {
            prop_assert_eq!(wire_index(&value), declaration_index(&value));
        }

        /// The derived encoder writes the pinned wire index for any payload.
        #[test]
        fn prop_encoder_writes_the_pinned_wire_index(value in tag_value()) {
            let config = bincode::config::standard();
            let bytes = bincode::encode_to_vec(&value, config)?;
            let (idx, _): (u32, usize) = bincode::decode_from_slice(&bytes, config)?;
            prop_assert_eq!(idx, wire_index(&value));
        }

        /// Decoding the derived encoding is the identity and consumes every byte.
        #[test]
        fn prop_decoder_roundtrips(value in tag_value()) {
            let config = bincode::config::standard();
            let bytes = bincode::encode_to_vec(&value, config)?;
            let (decoded, read): (TagValue, usize) =
                bincode::borrow_decode_from_slice_with_context(&bytes, config, ())?;
            prop_assert_eq!(&decoded, &value);
            prop_assert_eq!(read, bytes.len());
        }

        /// Every index past the last variant is rejected with the pinned range.
        #[test]
        fn prop_decoder_rejects_unknown_indices(unknown in (last_wire_index() + 1)..) {
            let config = bincode::config::standard();
            let bytes = bincode::encode_to_vec(unknown, config)?;
            let result: Result<(TagValue, usize), DecodeError> =
                bincode::borrow_decode_from_slice_with_context(&bytes, config, ());
            let Err(DecodeError::UnexpectedVariant { allowed, found, .. }) = result else {
                return Err(TestCaseError::fail("expected UnexpectedVariant"));
            };
            prop_assert_eq!(found, unknown);
            prop_assert_eq!(
                allowed,
                &AllowedEnumVariants::Range { min: 0, max: last_wire_index() }
            );
        }
    }
}
