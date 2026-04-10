//! encoding for regular expressions.
use std::{hash, ops::Deref};

use regex::Regex;
#[cfg(feature = "wasm")]
use tsify::Tsify;

/// A wrapper around `regex::Regex` that can be serialized and deserialized via bincode
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct EncodableRegex(
    #[serde(with = "serde_regex")]
    #[cfg_attr(feature = "wasm", tsify(type = "RegExp"))]
    Regex,
);

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
