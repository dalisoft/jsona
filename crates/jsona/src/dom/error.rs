use super::node::Key;
use crate::syntax::SyntaxElement;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("the syntax was not expected here: {syntax:#?}")]
    UnexpectedSyntax { syntax: SyntaxElement },
    #[error("the string contains invalid escape sequence(s)")]
    InvalidEscapeSequence { string: SyntaxElement },
    #[error("conflicting keys")]
    ConflictingKeys { key: Key, other: Key },
}
