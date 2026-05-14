//! Decoder for the `system_params` JS argument: an array of
//! `{ name, type, optional? }` objects, or `null`/`undefined`/missing.
//!
//! Malformed inputs degrade to an empty `Vec` rather than throwing — the
//! editor's diagnostics must never disappear because a host shipped a bad
//! registration.

use mpl_language_server::SystemParamSpec;
use wasm_bindgen::JsValue;

pub(crate) fn decode(value: JsValue) -> Vec<SystemParamSpec> {
    if value.is_null() || value.is_undefined() {
        return Vec::new();
    }
    serde_wasm_bindgen::from_value::<Vec<SystemParamSpec>>(value).unwrap_or_default()
}
