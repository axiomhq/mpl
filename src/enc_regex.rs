//! encoding for regular expressions.
use std::{hash, ops::Deref};

use regex::Regex;

use crate::ast;

/// A wrapper around `regex::Regex` that encodes as its pattern string.
#[derive(Debug, Clone)]
pub struct EncodableRegex(Regex);
impl From<ast::Regex> for EncodableRegex {
    fn from(regex: ast::Regex) -> Self {
        EncodableRegex(regex.into())
    }
}

impl PartialEq for EncodableRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl hash::Hash for EncodableRegex {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl EncodableRegex {
    /// Creates a new `EncodableRegex` from a regex pattern.
    pub fn new<S: AsRef<str>>(pattern: S) -> Result<Self, regex::Error> {
        Regex::new(pattern.as_ref()).map(EncodableRegex)
    }
}

impl From<Regex> for EncodableRegex {
    fn from(regex: Regex) -> Self {
        EncodableRegex(regex)
    }
}
impl AsRef<Regex> for EncodableRegex {
    fn as_ref(&self) -> &Regex {
        &self.0
    }
}

impl Deref for EncodableRegex {
    type Target = Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl serde::Serialize for EncodableRegex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for EncodableRegex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pattern = String::deserialize(deserializer)?;
        Regex::new(&pattern)
            .map(EncodableRegex)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "bincode")]
mod bincode_impls {
    use super::{EncodableRegex, Regex};

    impl bincode::Encode for EncodableRegex {
        fn encode<E: bincode::enc::Encoder>(
            &self,
            encoder: &mut E,
        ) -> Result<(), bincode::error::EncodeError> {
            self.0.as_str().encode(encoder)
        }
    }

    impl<Context> bincode::Decode<Context> for EncodableRegex {
        fn decode<D: bincode::de::Decoder>(
            decoder: &mut D,
        ) -> Result<Self, bincode::error::DecodeError> {
            let regex_str: String = bincode::Decode::decode(decoder)?;
            Regex::new(&regex_str)
                .map(EncodableRegex)
                .map_err(|_| bincode::error::DecodeError::Other("Invalid regex"))
        }
    }
    bincode::impl_borrow_decode!(EncodableRegex);
}

#[cfg(test)]
mod tests {
    use super::EncodableRegex;
    use test_case::test_case;

    #[test_case("^/api" ; "anchored literal")]
    #[test_case("[a-z]+" ; "character class")]
    #[test_case(r"\d{3}-\d{4}" ; "escapes and repetition")]
    #[test_case(r"\p{Greek}+" ; "unicode property class")]
    #[test_case("" ; "empty pattern")]
    fn json_round_trip_preserves_the_pattern(pattern: &str) {
        let regex = EncodableRegex::new(pattern).expect("valid pattern");
        let json = serde_json::to_string(&regex).expect("serialize");
        let back: EncodableRegex = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.as_str(), pattern);
    }

    /// The serialized form is the pattern string itself. The wasm JSON entry
    /// points and any stored query rely on that shape, so it is part of the
    /// contract rather than an implementation detail.
    #[test_case("^/api" ; "anchored literal")]
    #[test_case(r"\d{3}-\d{4}" ; "escapes and repetition")]
    fn serializes_as_a_bare_json_string(pattern: &str) {
        let regex = EncodableRegex::new(pattern).expect("valid pattern");

        assert_eq!(
            serde_json::to_string(&regex).expect("serialize"),
            serde_json::to_string(pattern).expect("serialize str")
        );
    }

    #[test]
    fn deserializing_an_invalid_pattern_fails() {
        assert!(serde_json::from_str::<EncodableRegex>(r#""[unclosed""#).is_err());
    }
}
