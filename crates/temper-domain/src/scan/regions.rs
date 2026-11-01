//! Regions of masked code: test-only code (`#[cfg(test)]` items, `#[test]`-style functions and
//! `proptest!` blocks, 1.x `testRegions`), compile-time code (1.x `constRegions`) and trait impl
//! bodies (1.x `traitImplRegions`).

use std::ops::RangeInclusive;

use regex::Regex;

use super::lexer::{match_brace, slice, statement_end};
use super::scanner::{ScannerSetupError, compile};
use crate::js::is_js_space;

/// Byte ranges of test-only code, both ends inclusive, in 1.x order: attribute regions, then
/// `proptest!` blocks. Ranges may nest.
#[derive(Debug)]
pub(crate) struct Regions(Vec<RangeInclusive<usize>>);

impl Regions {
    /// Is `offset` inside any region (1.x `inRegions`)?
    pub(crate) fn contains(&self, offset: usize) -> bool {
        self.0.iter().any(|region| region.contains(&offset))
    }
}

/// The compiled patterns behind [`test_regions`] and [`const_regions`].
#[derive(Debug)]
pub(crate) struct RegionPatterns {
    attr: Regex,
    test_word: Regex,
    not_test: Regex,
    proptest: Regex,
    const_block: Regex,
    const_item: Regex,
    impl_kw: Regex,
    for_kw: Regex,
}

impl RegionPatterns {
    /// Compiles the 1.x patterns.
    pub(crate) fn new() -> Result<Self, ScannerSetupError> {
        Ok(Self {
            attr: compile(
                "regions:attr",
                r"#\s*\[\s*(cfg\s*\(([^\]]*)\)|(?:[A-Za-z_][\w]*\s*::\s*)*(?:test|rstest|test_case)\b[^\]]*)\s*\]",
            )?,
            test_word: compile("regions:test-word", r"\btest\b")?,
            not_test: compile("regions:not-test", r"\bnot\s*\(\s*test\b")?,
            proptest: compile("regions:proptest", r"\bproptest\s*!\s*\{")?,
            const_block: compile("regions:const-block", r"\bconst\s*\{")?,
            const_item: compile("regions:const-item", r"\bconst\s+(?:_|[A-Za-z_]\w*)\s*:")?,
            impl_kw: compile("regions:impl", r"\bimpl\b")?,
            for_kw: compile("regions:for", r"\bfor\b")?,
        })
    }
}

/// The bodies of trait impls (`impl Trait for Type { .. }`) in masked `code`, whose method names
/// the trait fixes (1.x `traitImplRegions`). Each runs from `impl` to the brace closing the body.
pub(crate) fn trait_impl_regions(code: &str, patterns: &RegionPatterns) -> Regions {
    let regions = patterns.impl_kw.find_iter(code).filter_map(|m| {
        let open = impl_body(code, m.start().saturating_add(4))?;
        let header = slice(code, m.start(), open);
        // `for` not followed by `<` (a `for<'a>` bound), 1.x's `\bfor\b(?!\s*<)`.
        let trait_for = patterns.for_kw.find_iter(header).any(|f| {
            let rest = header.get(f.end()..).unwrap_or("");
            !rest.trim_start_matches(is_js_space).starts_with('<')
        });
        trait_for.then(|| m.start()..=match_brace(code, open))
    });
    Regions(regions.collect())
}

/// The `{` opening the body of the impl whose header continues at `from`: the first `{` outside
/// angle brackets (a `>` after `-` closes none). `None` when a `;` or `)` comes first there, or
/// nothing does.
fn impl_body(code: &str, from: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0_isize;
    for (at, &b) in bytes.iter().enumerate().skip(from) {
        match b {
            b'<' => depth = depth.saturating_add(1),
            b'>' if at.checked_sub(1).and_then(|p| bytes.get(p)) != Some(&b'-') => {
                depth = depth.saturating_sub(1);
            }
            b'{' if depth <= 0 => return Some(at),
            b';' | b')' if depth <= 0 => return None,
            _ => {}
        }
    }
    None
}

/// The regions of masked `code` evaluated at compile time (1.x `constRegions`): each inline
/// `const { .. }` block, then each `const NAME: T = ..;` item. A const generic parameter (after
/// `<` or `,`) is not one, and neither is a `const fn`.
pub(crate) fn const_regions(code: &str, patterns: &RegionPatterns) -> Regions {
    let blocks = patterns.const_block.find_iter(code).map(|m| {
        // The match ends with the block's `{`.
        m.start()..=match_brace(code, m.end().saturating_sub(1))
    });
    let items = patterns.const_item.find_iter(code).filter_map(|m| {
        let before = code.get(..m.start()).unwrap_or("");
        let prev = before.trim_end_matches(is_js_space).chars().next_back();
        let generic = matches!(prev, Some('<' | ','));
        (!generic).then(|| m.start()..=statement_end(code, m.start()))
    });
    Regions(blocks.chain(items).collect())
}

/// The test-only regions of masked `code`: each `#[cfg(..test..)]` (not `not(test)`) or
/// `#[test]`-style attribute through the end of the item that follows, then each `proptest!`
/// block.
pub(crate) fn test_regions(code: &str, patterns: &RegionPatterns) -> Regions {
    let mut regions = Vec::new();
    for caps in patterns.attr.captures_iter(code) {
        if let Some(cfg) = caps.get(2) {
            let cfg = cfg.as_str();
            if !patterns.test_word.is_match(cfg) || patterns.not_test.is_match(cfg) {
                continue;
            }
        }
        let attr = caps.get_match();
        if let Some(end) = item_end(code, attr.end()) {
            regions.push(attr.start()..=end);
        }
    }
    for block in patterns.proptest.find_iter(code) {
        // The match ends with the block's `{`.
        let open = block.end().saturating_sub(1);
        regions.push(block.start()..=match_brace(code, open));
    }
    Regions(regions)
}

/// The end of the item starting at `from`: the brace closing its body (the first `{` outside
/// brackets), or its `;` when it has no body. `None` when neither follows.
fn item_end(code: &str, from: usize) -> Option<usize> {
    let mut depth = 0_isize;
    for (at, &b) in code.as_bytes().iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' => depth = depth.saturating_add(1),
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth <= 0 => return Some(match_brace(code, at)),
            b';' if depth <= 0 => return Some(at),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::lexer::mask_rust;

    /// The regions as `(start, end)` pairs, the shape of the 1.x oracle's `[[a, b]]`.
    fn regions_of(src: &str) -> Vec<(usize, usize)> {
        let patterns = RegionPatterns::new().expect("the region patterns compile");
        test_regions(&mask_rust(src).code, &patterns)
            .0
            .into_iter()
            .map(RangeInclusive::into_inner)
            .collect()
    }

    #[test]
    fn cfg_test_module_and_test_fns_are_regions() {
        let src = "\npub fn ok() -> u8 { 1 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { assert_eq!(Some(ok()).unwrap(), 1); dbg!(1); }\n    #[test]\n    fn u() { unsafe { } }\n}";

        assert_eq!(
            regions_of(src),
            [(25, 178), (72, 138), (144, 176)],
            "the module, then each test fn"
        );
    }

    #[test]
    fn cfg_not_test_is_not_a_region() {
        let src = "#[cfg(not(test))]\nfn a() {}\n#[cfg(all(test, unix))]\nmod t { }";

        assert_eq!(
            regions_of(src),
            [(28, 60)],
            "only the cfg(all(test, ..)) item"
        );
    }

    #[test]
    fn bodiless_items_end_at_semicolon() {
        let src = "#[cfg(test)]\nmod tests;\n#[cfg(test)] static A: u8 = 1;\nfn z() {}";

        assert_eq!(regions_of(src), [(0, 22), (24, 53)], "each runs to its `;`");
    }

    #[test]
    fn path_and_rstest_and_test_case_attributes() {
        let src = "#[tokio::test]\nasync fn a() { x }\n#[rstest]\nfn b(#[values(1)] v: u8) { }\n#[test_case(1)]\nfn c() {}";

        assert_eq!(
            regions_of(src),
            [(0, 32), (34, 71), (73, 97)],
            "tokio::test, rstest and test_case each open a region"
        );
    }

    #[test]
    fn proptest_block_is_a_region_after_attribute_regions() {
        let src = "proptest! {\n    #[test]\n    fn p(x in 0..1u8) { }\n    fn q(y in any::<u8>()) {}\n}\nfn after() {}";

        assert_eq!(
            regions_of(src),
            [(16, 48), (0, 80)],
            "attribute regions come first, then proptest blocks"
        );
    }

    #[test]
    fn attribute_without_item_is_no_region() {
        assert_eq!(regions_of("#[test]"), [], "no item follows");
    }

    #[test]
    fn whitespace_inside_attribute() {
        assert_eq!(
            regions_of("# [ cfg ( test ) ]\nmod t {}"),
            [(0, 26)],
            "spaces inside the attribute are allowed"
        );
    }

    #[test]
    fn unbalanced_body_runs_to_end() {
        assert_eq!(
            regions_of("#[cfg(test)]\nmod t { fn a() {"),
            [(0, 28)],
            "an unclosed body runs to the last offset"
        );
    }

    #[test]
    fn const_regions_skip_const_generics() {
        let src = "const A: u8 = 1;\nfn f<const N: usize>() { const { assert!(true) }; }\nfn g(x: u8, const B: u8) {}\nconst _: () = assert!(true);";
        let patterns = RegionPatterns::new().expect("the region patterns compile");

        let regions: Vec<(usize, usize)> = const_regions(&mask_rust(src).code, &patterns)
            .0
            .into_iter()
            .map(RangeInclusive::into_inner)
            .collect();

        assert_eq!(
            regions,
            [(42, 64), (0, 15), (97, 124)],
            "blocks first, then items; const generic parameters are skipped"
        );
    }

    #[test]
    fn trait_impl_regions_follow_1x() {
        let src = "impl<T> for<'a> X {}\nimpl Clock for Sys { fn now() {} }\nimpl Sys { fn t() {} }\nimpl<F: Fn() -> u8> Clock for F { }\nfn f(x: impl Fn() -> u8) {}\nimpl Tr for\n  Y {}";
        let patterns = RegionPatterns::new().expect("the region patterns compile");

        let regions: Vec<(usize, usize)> = trait_impl_regions(&mask_rust(src).code, &patterns)
            .0
            .into_iter()
            .map(RangeInclusive::into_inner)
            .collect();

        assert_eq!(
            regions,
            [(21, 54), (79, 113), (143, 160)],
            "trait impls only: not `for<'a>`, an inherent impl or `impl Fn` in a parameter"
        );
    }

    #[test]
    fn regions_contain_both_ends() {
        let regions = Regions(std::iter::once(25..=178).collect());

        assert_eq!(
            [24, 25, 178, 179].map(|o| regions.contains(o)),
            [false, true, true, false],
            "both ends are inside"
        );
    }
}
