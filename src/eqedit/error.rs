//! Error type for EqEdit → LaTeX conversion.

use thiserror::Error;

/// Errors produced by [`crate::eqedit::convert`].
///
/// Conversion is permissive: unknown commands and identifiers never error.
/// The only failure mode is structurally irrecoverable input.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EqError {
    /// Braces in the script are not balanced (an unmatched `{` or `}`),
    /// and recovery is not possible.
    #[error("unbalanced braces in EqEdit script")]
    UnbalancedBrace,
}
