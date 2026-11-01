//! The test, `#[ignore]` and `.clone()` counts of masked code (1.x `countTests`,
//! `countIgnored` and the scan's clone count), which detect deleted or ignored tests (H-12).

use regex::Regex;

use super::lexer::{match_brace, slice};
use super::scanner::{ScannerSetupError, compile};
use crate::js::js_trim;

/// The compiled patterns behind the counts.
#[derive(Debug)]
pub(crate) struct CountPatterns {
    test_attr: Regex,
    proptest: Regex,
    fn_name: Regex,
    ignore: Regex,
    clone_call: Regex,
}

impl CountPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            test_attr: compile(
                "counts:test-attr",
                r"#\s*\[\s*(?:[A-Za-z_][\w]*\s*::\s*)*(?:test|rstest|test_case)\b[^\]]*\]",
            )?,
            proptest: compile("counts:proptest", r"\bproptest\s*!\s*\{")?,
            fn_name: compile("counts:fn-name", r"\bfn\s+\w+")?,
            ignore: compile(
                "counts:ignore",
                r"#\s*\[\s*(?:ignore\b|cfg_attr\s*\(([^\]]*?),\s*ignore\b)",
            )?,
            clone_call: compile("counts:clone", r"\.\s*clone\s*\(\s*\)")?,
        })
    }
}

/// The number of tests in masked `code`: `#[test]`-style attributes, plus the `fn`s inside
/// `proptest!` blocks (or the number of blocks when they hold no `fn`).
pub(crate) fn count_tests(code: &str, patterns: &CountPatterns) -> usize {
    let attrs = patterns.test_attr.find_iter(code).count();
    let (blocks, prop_fns) =
        patterns
            .proptest
            .find_iter(code)
            .fold((0_usize, 0_usize), |(blocks, fns), block| {
                // The match ends with the block's `{`.
                let open = block.end().saturating_sub(1);
                let body = slice(code, open, match_brace(code, open));
                let found = patterns.fn_name.find_iter(body).count();
                (blocks.saturating_add(1), fns.saturating_add(found))
            });
    attrs.saturating_add(if prop_fns > 0 { prop_fns } else { blocks })
}

/// The number of `#[ignore]` attributes in masked `code`, reasoned or not, including a top-level
/// `cfg_attr(.., ignore)`, but not `cfg_attr(miri, ignore)`, which TST-17 requires.
pub(crate) fn count_ignored(code: &str, patterns: &CountPatterns) -> usize {
    patterns
        .ignore
        .captures_iter(code)
        .filter(|caps| match caps.get(1) {
            None => true,
            Some(predicate) => {
                top_level_cfg_attr_arg(caps.get_match().as_str())
                    && js_trim(predicate.as_str()) != "miri"
            }
        })
        .count()
}

/// The number of `.clone()` calls in masked `code`.
pub(crate) fn count_clones(code: &str, patterns: &CountPatterns) -> usize {
    patterns.clone_call.find_iter(code).count()
}

/// For matched text `#[cfg_attr(<pred>, .., ignore`: is the last argument a top-level argument
/// of `cfg_attr`, not nested in another attribute such as `educe(Debug, ignore)`?
pub(crate) fn top_level_cfg_attr_arg(text: &str) -> bool {
    let Some(at) = text.find("cfg_attr") else {
        return true;
    };
    // 1.x `indexOf('(', at) + 1`: from the start of the text when no `(` follows.
    let args = text
        .get(at..)
        .and_then(|rest| rest.find('('))
        .map_or(0, |open| at.saturating_add(open).saturating_add(1));
    let depth = text
        .get(args..)
        .unwrap_or("")
        .chars()
        .fold(0_isize, |depth, c| match c {
            '(' => depth.saturating_add(1),
            ')' => depth.saturating_sub(1),
            _ => depth,
        });
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::lexer::mask_rust;

    fn patterns() -> CountPatterns {
        CountPatterns::new().expect("the count patterns compile")
    }

    const PROPTEST_SRC: &str = "proptest! {\n    #[test]\n    fn p(x in 0..1u8) { }\n    fn q(y in any::<u8>()) {}\n}\nfn after() {}";
    const IGNORE_SRC: &str = "#[test]\n#[ignore = \"slow\"]\nfn a() {}\n#[test]\n#[cfg_attr(miri, ignore = \"spawns a process\")]\nfn b() {}\n#[test]\n#[cfg_attr(windows, ignore = \"unix sockets\")]\nfn c() {}\n#[test]\n#[ignore]\nfn d() {}";

    #[test]
    fn counts_two_tests_in_the_cfg_test_module() {
        let src = "\npub fn ok() -> u8 { 1 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }\n    #[test]\n    fn u() { unsafe { } }\n}";

        assert_eq!(
            count_tests(&mask_rust(src).code, &patterns()),
            2,
            "two #[test] fns"
        );
    }

    #[test]
    fn counts_reasoned_and_unreasoned_ignores_except_miri() {
        let code = mask_rust(IGNORE_SRC).code;

        assert_eq!(
            count_ignored(&code, &patterns()),
            3,
            "cfg_attr(miri, ignore) is not counted"
        );
        assert_eq!(count_tests(&code, &patterns()), 4, "four #[test] fns");
    }

    #[test]
    fn nested_educe_ignore_is_not_counted() {
        let src = "#[derive(Educe)]\n#[educe(Debug, ignore)]\nstruct S;\n#[cfg_attr(feature = \"x\", educe(Debug, ignore))]\nstruct T;";

        assert_eq!(
            count_ignored(&mask_rust(src).code, &patterns()),
            0,
            "an ignore nested in another attribute is not a test ignore"
        );
    }

    #[test]
    fn miri_predicate_is_trimmed_and_exact() {
        let src =
            "#[cfg_attr(not(miri), ignore)]\nfn a() {}\n#[cfg_attr( miri , ignore)]\nfn b() {}";

        assert_eq!(
            count_ignored(&mask_rust(src).code, &patterns()),
            1,
            "not(miri) counts; a spaced miri does not"
        );
    }

    #[test]
    fn proptest_fns_count_as_tests() {
        let counts = [
            PROPTEST_SRC,
            "proptest! { fn p(x in 0..1u8) {} fn q(y in 0..2u8) {} }",
            "#[test]",
        ]
        .map(|src| count_tests(&mask_rust(src).code, &patterns()));

        assert_eq!(
            counts,
            [3, 2, 1],
            "attributes plus the fns inside proptest! blocks"
        );
    }

    #[test]
    fn counts_clone_calls() {
        let src = "let a = b.clone(); c . clone ( ); d.clone_from(e);";

        assert_eq!(
            count_clones(&mask_rust(src).code, &patterns()),
            2,
            "spaced calls count, clone_from does not"
        );
    }
}
