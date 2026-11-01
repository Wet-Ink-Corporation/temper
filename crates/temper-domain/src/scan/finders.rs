//! The hand-written finders (1.x `find(env)` rules), one module per concept.

pub(crate) mod expect_attrs;

use std::borrow::Cow;

use super::hit::Severity;

/// One match a rule reports (1.x `{ offset, severity?, msg?, fix?, exception? }`).
#[derive(Debug)]
pub(crate) struct Found {
    /// Byte offset of the match in the masked code.
    pub(crate) offset: usize,
    /// A severity that overrides the rule's context table.
    pub(crate) severity: Option<Severity>,
    pub(crate) msg: Option<Cow<'static, str>>,
    pub(crate) fix: Option<&'static str>,
    /// The lint an `#[expect(..)]` names.
    pub(crate) exception: Option<String>,
}

impl Found {
    /// A plain match at `offset`, reported with the rule's own texts and context severity.
    pub(crate) const fn at(offset: usize) -> Self {
        Self {
            offset,
            severity: None,
            msg: None,
            fix: None,
            exception: None,
        }
    }
}
