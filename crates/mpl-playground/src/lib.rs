//! MPL playground: step-based parser and in-memory interpreter.
#![deny(
    warnings,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::large_futures,
    missing_docs
)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::match_same_arms,
    clippy::if_not_else,
    clippy::unnecessary_wraps,
    clippy::must_use_candidate,
    clippy::assigning_clones,
    clippy::collapsible_if,
    clippy::only_used_in_recursion,
    clippy::single_match_else
)]

pub mod interpreter;
pub mod steps;
