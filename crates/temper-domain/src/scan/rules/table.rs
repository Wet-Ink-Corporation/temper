//! The rules, in 1.x table order. Patterns are 1.x's JS regex sources verbatim, translated when
//! `Scanner::new` compiles them, so each entry diffs line by line against `scan.mjs`. A pattern
//! whose 1.x source has a look-around holds the rewritten source, with the original on a
//! `1.x:` line above it.
//!
//! One file for the whole table, over ARC-06's ~400 lines: one ordered slice keeps the 1.x diff
//! and the order, which splitting it would lose (decision 1b-7, wi-cbeff6).

use super::{FinderKind, HARD, Keep, Look, Matcher, OFF, RuleSpec, SOFT, ctx, re};

/// The rules, in 1.x table order. Phases 1a and 1b port these; 1c adds the finders in place.
pub(crate) const RULES: &[RuleSpec] = &[
    RuleSpec {
        key: "unsafe",
        id: "H-01",
        contexts: ctx(HARD, HARD, HARD, HARD),
        msg: "`unsafe` is forbidden in this workspace",
        fix: "Find a safe API or restructure. A genuine need (FFI) is a human decision: isolate it in a -sys crate and change the lint config with approval.",
        fix_build: None,
        lints: &[],
        matcher: Matcher::Regex(re(r"\bunsafe\b")),
    },
    RuleSpec {
        key: "unwrap",
        id: "H-02",
        contexts: ctx(HARD, HARD, HARD, OFF),
        msg: "unwrap/expect in production code",
        fix: "Propagate with `?`, convert with `ok_or_else`/`let ... else`, or make the invariant a type. See references/errors.md \"Eliminating unwrap/expect\".",
        fix_build: None,
        lints: &["clippy::unwrap_used", "clippy::expect_used"],
        matcher: Matcher::Regex(re(
            r#"\.\s*(?:unwrap\s*\(\s*\)|unwrap_err\s*\(\s*\)|unwrap_unchecked\s*\(|expect\s*\(\s*"|expect_err\s*\(\s*")|\b(?:Option|Result)\s*::\s*(?:unwrap|expect|unwrap_unchecked|unwrap_err|expect_err)\b"#,
        )),
    },
    RuleSpec {
        key: "panic",
        id: "H-03",
        contexts: ctx(HARD, HARD, HARD, OFF),
        msg: "panicking macro in production code",
        fix: "Return an error, or restructure types so the case cannot occur (exhaustive match, newtype, NonZero, typestate). Unfinished work is `Status: WIP`, never `todo!()`.",
        fix_build: None,
        lints: &[
            "clippy::panic",
            "clippy::todo",
            "clippy::unimplemented",
            "clippy::unreachable",
        ],
        matcher: Matcher::Regex(re(
            r"\b(?:panic|todo|unimplemented|unreachable)\s*!|\bprocess\s*::\s*abort\s*\(",
        )),
    },
    RuleSpec {
        key: "assert-in-prod",
        id: "H-03",
        contexts: ctx(SOFT, SOFT, OFF, OFF),
        msg: "`assert!` panics in production code",
        fix: "Use `debug_assert!` for internal sanity checks, a compile-time `const { assert!(..) }` for constants, or return an error for conditions callers can trigger (H-03).",
        fix_build: None,
        lints: &[],
        matcher: Matcher::Regex(
            re(r"\b(?:assert|assert_eq|assert_ne)\s*!").keep(Keep::OutsideConst),
        ),
    },
    RuleSpec {
        key: "dbg",
        id: "H-06",
        contexts: ctx(HARD, HARD, HARD, HARD),
        msg: "`dbg!` left in code",
        fix: "Remove it. Use `tracing::debug!` with structured fields for diagnostics that should stay.",
        fix_build: None,
        lints: &["clippy::dbg_macro"],
        matcher: Matcher::Regex(re(r"\bdbg\s*!")),
    },
    RuleSpec {
        key: "print",
        id: "H-06",
        contexts: ctx(HARD, HARD, SOFT, OFF),
        msg: "print macro in production code",
        fix: "Libraries: use `tracing`. Binaries: write through a locked handle, `writeln!(io::stdout().lock(), ...)?`. It returns an error on a closed pipe where `println!` would panic.",
        fix_build: Some(
            "Build scripts emit directives with `writeln!(io::stdout().lock(), \"cargo::rerun-if-changed=..\")?` from a `main` that returns `Result` (ARC-17, H-06).",
        ),
        lints: &["clippy::print_stdout", "clippy::print_stderr"],
        matcher: Matcher::Regex(re(r"\be?print(?:ln)?\s*!")),
    },
    RuleSpec {
        key: "as-cast",
        id: "H-11",
        contexts: ctx(HARD, HARD, HARD, SOFT),
        msg: "numeric `as` cast (can truncate, wrap or lose sign)",
        fix: "Use `From`/`TryFrom` (`u64::from(x)`, `u32::try_from(n)?`) or `checked_*`/`saturating_*`.",
        fix_build: None,
        lints: &[
            "clippy::as_conversions",
            "clippy::cast_possible_truncation",
            "clippy::cast_sign_loss",
            "clippy::cast_possible_wrap",
            "clippy::cast_precision_loss",
            "clippy::cast_lossless",
        ],
        matcher: Matcher::Regex(re(
            r"\bas\s+(?:u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64|char)\b",
        )),
    },
    RuleSpec {
        key: "ptr-param",
        id: "OWN-03",
        contexts: ctx(HARD, HARD, HARD, SOFT),
        msg: "borrowed owning container (`&String`, `&Vec<T>`, `&Box<T>`, `&PathBuf`)",
        fix: "Borrow the view instead: `&str`, `&[T]`, `&T`, `&Path`, `&OsStr`.",
        fix_build: None,
        lints: &["clippy::ptr_arg", "clippy::borrowed_box"],
        matcher: Matcher::Regex(re(r":\s*&\s*(?:String|Vec\s*<|Box\s*<|PathBuf|OsString)\b")),
    },
    RuleSpec {
        key: "ignore-no-reason",
        id: "H-12",
        contexts: ctx(HARD, HARD, HARD, HARD),
        msg: "`#[ignore]` or `cfg_attr(.., ignore)` without a reason",
        fix: "Do not disable tests to get green (H-12). If a test is slow, needs external resources or cannot run under Miri, write `#[ignore = \"reason\"]` or `#[cfg_attr(miri, ignore = \"reason\")]` and tell the human.",
        fix_build: None,
        lints: &[],
        // 1.x: #\s*\[\s*(?:cfg_attr\s*\([^\]]*?,\s*)?ignore\s*(?=[\],)])
        // The look-ahead's char is consumed and the keep drops it; no match starts with it.
        matcher: Matcher::Regex(
            re(r"#\s*\[\s*(?:cfg_attr\s*\([^\]]*?,\s*)?ignore\s*[\],)]").keep(Keep::TopLevelIgnore),
        ),
    },
    RuleSpec {
        key: "thread-spawn",
        id: "ASY-15",
        contexts: ctx(HARD, HARD, HARD, SOFT),
        msg: "`thread::spawn` panics when the OS refuses a thread",
        fix: "Use `thread::Builder::new().name(..).spawn(..)?` (or `spawn_scoped`), propagate the `io::Error`, and keep the `JoinHandle` (ASY-15).",
        fix_build: None,
        lints: &["clippy::disallowed_methods"],
        matcher: Matcher::Regex(re(r"\bthread\s*::\s*spawn\s*\(")),
    },
    RuleSpec {
        key: "extern-crate",
        id: "MOD-01",
        contexts: ctx(SOFT, SOFT, SOFT, SOFT),
        msg: "`extern crate` is unnecessary in edition 2018+",
        fix: "Remove it; use `use` paths. Only `extern crate alloc;`/`extern crate std;` in a `no_std` crate are needed (MOD-01).",
        fix_build: None,
        lints: &[],
        // 1.x: \bextern\s+crate\s+(?!(?:alloc|std)\s*;|self\s+as\b)
        matcher: Matcher::Regex(re(r"\bextern\s+crate\s+").look(Look::NeededExternCrate)),
    },
    RuleSpec {
        key: "expect-attr",
        id: "H-05",
        contexts: ctx(HARD, HARD, HARD, HARD),
        msg: "`#[expect]` lint exception",
        fix: "Exception requests need human approval (H-05).",
        fix_build: None,
        lints: &[],
        matcher: Matcher::Find(FinderKind::ExpectAttrs),
    },
];
