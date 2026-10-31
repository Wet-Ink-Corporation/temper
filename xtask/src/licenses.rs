//! `cargo xtask check-licenses`: every crate ships both licence texts, equal to the root's.
//!
//! `cargo package` packs only the files inside a crate's directory, so each crate keeps plain
//! copies of the root `LICENSE` (as `LICENSE-MIT`) and `LICENSE-APACHE` (port plan §1.2).

use std::fmt;

/// Each licence copy a crate holds, with the root file it must equal.
pub(crate) const COPIES: [LicenseCopy; 2] = [
    LicenseCopy {
        in_crate: "LICENSE-MIT",
        at_root: "LICENSE",
    },
    LicenseCopy {
        in_crate: "LICENSE-APACHE",
        at_root: "LICENSE-APACHE",
    },
];

/// One licence file a crate copies from the root.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LicenseCopy {
    /// Its name in the crate directory.
    pub(crate) in_crate: &'static str,
    /// The root file it copies.
    pub(crate) at_root: &'static str,
}

/// A crate's licence copy that is not what the root holds.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The crate has no such file.
    Missing { krate: String, file: &'static str },
    /// The file differs from the root's.
    Differs {
        krate: String,
        file: &'static str,
        root: &'static str,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { krate, file } => write!(f, "{krate}: {file} is missing"),
            Self::Differs { krate, file, root } => {
                write!(f, "{krate}: {file} differs from the root {root}")
            }
        }
    }
}

/// Compare one crate's copy (`None` when the file is absent) with the root text.
pub(crate) fn compare(
    krate: &str,
    copy: LicenseCopy,
    root_text: &str,
    found: Option<&str>,
) -> Option<Problem> {
    match found {
        None => Some(Problem::Missing {
            krate: krate.to_owned(),
            file: copy.in_crate,
        }),
        Some(text) if text != root_text => Some(Problem::Differs {
            krate: krate.to_owned(),
            file: copy.in_crate,
            root: copy.at_root,
        }),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{COPIES, Problem, compare};

    #[test]
    fn an_equal_copy_has_no_problem() {
        assert_eq!(
            compare("temper-cli", COPIES[0], "MIT text", Some("MIT text")),
            None,
            "equal texts"
        );
    }

    #[test]
    fn a_missing_copy_is_reported() {
        assert_eq!(
            compare("temper-app", COPIES[1], "Apache text", None),
            Some(Problem::Missing {
                krate: "temper-app".to_owned(),
                file: "LICENSE-APACHE"
            }),
            "missing file",
        );
    }

    #[test]
    fn a_differing_copy_names_the_root_file() {
        let problem = compare(
            "temper-host",
            COPIES[0],
            "MIT text",
            Some("MIT text, edited"),
        );
        assert_eq!(
            problem,
            Some(Problem::Differs {
                krate: "temper-host".to_owned(),
                file: "LICENSE-MIT",
                root: "LICENSE"
            }),
            "differing file",
        );
        assert_eq!(
            problem.map(|p| p.to_string()).as_deref(),
            Some("temper-host: LICENSE-MIT differs from the root LICENSE"),
            "message",
        );
    }
}
