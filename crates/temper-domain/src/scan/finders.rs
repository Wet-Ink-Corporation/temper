//! The hand-written finders (1.x `find(env)` rules), one module per concept.

pub(crate) mod comments;
pub(crate) mod empty_errs;
pub(crate) mod erased_errors;
pub(crate) mod expect_attrs;
pub(crate) mod forbid_overrides;
pub(crate) mod serde_hashmaps;
pub(crate) mod spin_locks;
pub(crate) mod struct_bounds;
pub(crate) mod test_branches;

use std::borrow::Cow;

use self::comments::CommentPatterns;
use self::empty_errs::IfLetErrPatterns;
use self::erased_errors::ErasedPatterns;
use self::expect_attrs::ExpectPatterns;
use self::forbid_overrides::ForbidPatterns;
use self::serde_hashmaps::SerdePatterns;
use self::spin_locks::SpinPatterns;
use self::struct_bounds::StructBoundPatterns;
use self::test_branches::TestBranchPatterns;
use super::env::Env;
use super::hit::Severity;
use super::rules::FinderKind;
use super::scanner::ScannerSetupError;

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

/// Every finder's compiled patterns, one owner each.
#[derive(Debug)]
pub(crate) struct FinderPatterns {
    /// Also read by the `#[expect]` lookup that turns a hard hit into an exception request.
    pub(crate) expect: ExpectPatterns,
    if_let_err: IfLetErrPatterns,
    spin: SpinPatterns,
    serde: SerdePatterns,
    test_branch: TestBranchPatterns,
    struct_bounds: StructBoundPatterns,
    forbid: ForbidPatterns,
    erased: ErasedPatterns,
    comments: CommentPatterns,
}

impl FinderPatterns {
    /// Compiles every finder's 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            expect: ExpectPatterns::new()?,
            if_let_err: IfLetErrPatterns::new()?,
            spin: SpinPatterns::new()?,
            serde: SerdePatterns::new()?,
            test_branch: TestBranchPatterns::new()?,
            struct_bounds: StructBoundPatterns::new()?,
            forbid: ForbidPatterns::new()?,
            erased: ErasedPatterns::new()?,
            comments: CommentPatterns::new()?,
        })
    }

    /// Pushes the matches of finder `kind` onto `out`.
    pub(crate) fn find(&self, kind: FinderKind, env: &Env<'_>, out: &mut Vec<Found>) {
        match kind {
            FinderKind::EmptyIfLetErr => empty_errs::find(env, &self.if_let_err, out),
            FinderKind::SpinLocks => spin_locks::find(env, &self.spin, out),
            FinderKind::SerializedHashMaps => serde_hashmaps::find(env, &self.serde, out),
            FinderKind::TestBranches => test_branches::find(env, &self.test_branch, out),
            FinderKind::StructBounds => struct_bounds::find(env, &self.struct_bounds, out),
            FinderKind::ExpectAttrs => expect_attrs::find(env, &self.expect, out),
            FinderKind::ForbidOverrides => forbid_overrides::find(env, &self.forbid, out),
            FinderKind::ErasedErrors => erased_errors::find(env, &self.erased, out),
            FinderKind::TodoComments => comments::find_todos(env, &self.comments, out),
            FinderKind::CommentedCode => comments::find_commented_code(env, &self.comments, out),
        }
    }
}
