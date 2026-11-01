//! The doc-comment rules: `doc-unwrap` (1.x tests 14 and 15) and the oracle cases for both doc
//! rules. 1.x's `doc-arg-sections` test stays at its 1.x place in `api.rs`.

use super::{has, hits_of, sev_of};
use crate::scan::{CodeKind, Severity};

const BAD_DOCTEST: &str = "\n/// Parses an id.\n///\n/// ```\n/// let id: AccountId = \"acc-42\".parse().unwrap();\n/// # let other: AccountId = \"acc-7\".parse().expect(\"valid\");\n/// ```\npub fn parse() {}\n";

#[test]
fn doc_unwrap_unwrap_expect_panic_in_doctest_fences() {
    let hits: Vec<(Severity, usize)> = has(BAD_DOCTEST, "doc-unwrap", CodeKind::Prod)
        .iter()
        .map(|h| (h.severity(), h.line().get()))
        .collect();
    assert_eq!(
        hits,
        [(Severity::Hard, 5), (Severity::Hard, 6)],
        "a visible and a hidden line"
    );
    let one_each: [(&str, CodeKind, &str, &[&str]); 5] = [
        (
            "//! ```no_run\n//! if bad { panic!(\"boom\") }\n//! ```\n",
            CodeKind::Prod,
            "an inner no_run fence",
            &["hard"],
        ),
        (
            "/**\n * ```\n * let v = f().unwrap();\n * ```\n */\npub fn g() {}",
            CodeKind::Prod,
            "a block doc comment",
            &["hard"],
        ),
        (
            "#[cfg(test)]\nmod tests {\n    /// ```\n    /// x.unwrap();\n    /// ```\n    fn helper() {}\n}",
            CodeKind::Prod,
            "a doc comment in a test region",
            &["soft"],
        ),
        (
            "//! ```\n//! let cfg = Config::load().unwrap();\n//! ```\nfn main() {}",
            CodeKind::Bin,
            "rustdoc runs no doctests in binaries",
            &["soft"],
        ),
        (
            "//! ```\n//! let cfg = Config::load().unwrap();\n//! ```\nfn main() {}",
            CodeKind::Example,
            "rustdoc runs no doctests in examples",
            &["soft"],
        ),
    ];
    for (src, kind, why, want) in one_each {
        assert_eq!(sev_of(src, "doc-unwrap", kind), want, "{why}");
    }
    assert_eq!(
        has(
            "/// ```should_panic\n/// let v: Vec<u8> = Vec::new();\n/// v.first().unwrap();\n/// ```\npub fn f() {}",
            "doc-unwrap",
            CodeKind::Prod
        ),
        [],
        "a should_panic fence shows a panic on purpose"
    );
    let crlf = BAD_DOCTEST.replace('\n', "\r\n");
    let lines: Vec<usize> = has(&crlf, "doc-unwrap", CodeKind::Prod)
        .iter()
        .map(|h| h.line().get())
        .collect();
    assert_eq!(lines, [5, 6], "CRLF sources");
}

/// 1.x test 15's scan half; its `isRustFence` half is `is_rust_fence_tokens`, beside the
/// private function (decision 1d-6).
#[test]
fn doc_unwrap_idiomatic_doctests_and_non_rust_fences_are_clean() {
    // ledger-domain/src/account_id.rs
    let good = "\n/// # Examples\n///\n/// ```\n/// # fn main() -> Result<(), ledger_domain::ParseAccountIdError> {\n/// let id: ledger_domain::AccountId = \"acc-42\".parse()?;\n/// assert_eq!(id.as_str(), \"acc-42\");\n/// # Ok(())\n/// # }\n/// ```\n///\n/// Never call `.unwrap()` here; `panic!` is banned too.\n///\n/// ```text\n/// thread 'main' panicked at src/main.rs: called `Option::unwrap()` on a `None` value\n/// ```\n///\n/// ```compile_fail\n/// let x: u8 = None.unwrap();\n/// ```\n///\n/// ```rust,ignore\n/// let x = y.unwrap();\n/// ```\n///\n/// ```toml\n/// expect = \"x.unwrap()\"\n/// ```\n///\n/// ```\n/// let s = \"call .unwrap() and panic!() elsewhere\";\n/// # Ok::<(), std::io::Error>(())\n/// ```\n#[derive(Debug)]\npub struct AccountId(String);\n";

    assert_eq!(
        has(good, "doc-unwrap", CodeKind::Prod),
        [],
        "idiomatic doctests and non-Rust fences"
    );
}

#[test]
fn doc_unwrap_like_1x() {
    let cases: [(&str, &str, &[&str]); 6] = [
        (
            "/// ```\n/// #  let a = x.unwrap();\n/// # let b = y.unwrap();\n///     #\n/// #[derive(Debug)] struct S;\n/// let s = \"z.unwrap()\";\n/// let r = r\"\n/// w.unwrap()\n/// \";\n/// ```\npub fn f() {}",
            "hidden lines are checked; strings are masked",
            &["H-02:hard@2", "H-02:hard@3"],
        ),
        (
            "/// ~~~rust,should_panic\n/// a.unwrap();\n/// ~~~\n/// ```edition2021, no_run\n/// b.expect(\"x\");\n/// ```\n/// ```rust ignore\n/// c.unwrap();\n/// ```\n/// ```\n/// todo!();\npub fn f() {}",
            "fence tokens, and a fence left open at the end of the docs",
            &["H-02:hard@5", "H-02:hard@11"],
        ),
        (
            "//! ```\n/// a.unwrap();\n//! ```\nfn f() {}",
            "inner and outer runs do not merge",
            &[],
        ),
        (
            "/**\n * ```\n *  d.unwrap();\n\t* e.unwrap();\n f.unwrap();\n */\npub fn f() {}",
            "block comment leads",
            &["H-02:hard@3", "H-02:hard@4", "H-02:hard@5"],
        ),
        (
            "/// ```\n/// let s = \"\n/// x.unwrap()\";\n/// y.unwrap();\n/// ```\nfn f(){}",
            "a string spans fence lines",
            &["H-02:hard@4"],
        ),
        (
            "#[doc = \"```\"]\n/// x.unwrap();\npub fn f() {}",
            "attribute docs are not comments",
            &[],
        ),
    ];

    for (src, why, want) in cases {
        assert_eq!(hits_of(src, "doc-unwrap", CodeKind::Prod), want, "{why}");
    }
}

#[test]
fn doc_arg_sections_like_1x() {
    let src = "/// **Returns:**\n/// ## Parameters\n///   Return value  \n/// # Arguments:\n/// Returns the sum.\n/// ```\n/// # Returns\n/// ```\n/// #######  Params\n/** Params */\n// # Returns\npub fn f() {}";
    let soft = [
        "API-15:soft@1",
        "API-15:soft@2",
        "API-15:soft@3",
        "API-15:soft@4",
        "API-15:soft@10",
    ];

    let by_kind = [
        CodeKind::Prod,
        CodeKind::Bin,
        CodeKind::Example,
        CodeKind::Test,
    ]
    .map(|kind| hits_of(src, "doc-arg-sections", kind));

    assert_eq!(
        by_kind,
        [soft.to_vec(), soft.to_vec(), Vec::new(), Vec::new()],
        "prose headings in prod and bin; off in examples and tests"
    );
}
