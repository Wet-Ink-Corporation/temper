//! 1.x tests 10 to 13: `assert-in-prod`, `ignore-no-reason`, `extern-crate` and the build-script
//! fix texts.

use super::{has, hit, hits_of, scan_as, sev_of};
use crate::scan::{CodeKind, FileRole, Severity, is_build_script};

#[test]
fn assert_in_prod_compile_time_assertions_are_exempt() {
    let key = "assert-in-prod";

    assert_eq!(
        sev_of(
            "pub fn f(x: u8) { assert!(x > 0, \"x must be positive\"); }",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a run-time assert is advisory"
    );
    assert_eq!(
        has(
            "const _: () = assert!(size_of::<Event>() <= 128, \"Event grew past two cache lines\");",
            key,
            CodeKind::Prod
        ),
        [],
        "PERF-05 size pin"
    );
    assert_eq!(
        has(
            "pub fn chunk<const N: usize>(xs: &[u8]) -> usize {\n    const { assert!(N > 0, \"N must be positive\") };\n    xs.len() / N\n}",
            key,
            CodeKind::Prod
        ),
        [],
        "MOD-10 inline const block"
    );
    assert_eq!(
        sev_of(
            "impl<const N: usize> Ring<N> {\n    pub fn push(&mut self) { assert!(self.len < N, \"full\"); }\n}",
            key,
            CodeKind::Prod
        ),
        ["soft"],
        "a const generic parameter is not a const context"
    );
}

#[test]
fn assert_in_prod_const_contexts() {
    let src = "const A: u8 = { assert!(true); 1 };\nfn f<const N: usize>() { assert!(N > 0); }\nfn g(a: u8,\n const B: u8) { assert!(a > 0); }\nconst fn h() { assert!(true); }";

    assert_eq!(
        hits_of(src, "assert-in-prod", CodeKind::Prod),
        ["H-03:soft@2", "H-03:soft@4", "H-03:soft@5"],
        "const items and blocks are exempt; const generics and const fn are not"
    );
}

#[test]
fn ignore_no_reason_also_catches_cfg_attr_ignore() {
    let key = "ignore-no-reason";
    let flagged = [
        "#[ignore]\n#[test]\nfn slow() {}",
        "#[cfg_attr(miri, ignore)]\n#[test]\nfn spawns() {}",
        "#[cfg_attr(any(miri, all(unix, not(target_os = \"linux\"))), ignore)]\n#[test]\nfn t() {}",
    ];
    let clean = [
        "#[cfg_attr(miri, ignore = \"Miri cannot spawn processes\")]\n#[test]\nfn cli() {}",
        "#[ignore = \"needs a network\"]\n#[test]\nfn net() {}",
        "#[derive(Debug)]\n#[cfg_attr(feature = \"educe\", educe(Debug, ignore))]\nstruct S;",
    ];

    for s in flagged {
        assert_eq!(sev_of(s, key, CodeKind::Test), ["hard"], "{s}");
    }
    for s in clean {
        assert_eq!(has(s, key, CodeKind::Test), [], "{s}");
    }
}

#[test]
fn ignore_no_reason_backtracks_like_js() {
    let key = "ignore-no-reason";

    assert_eq!(
        hits_of(
            "#[cfg_attr(x, ignorefoo, ignore)]\n#[test]\nfn t() {}",
            key,
            CodeKind::Test
        ),
        ["H-12:hard@1"],
        "JS backtracks the lazy `[^\\]]*?` past `ignorefoo`"
    );
    assert_eq!(
        hits_of(
            "#[ignore ]\n#[test]\nfn t() {}\n#[cfg_attr(miri, ignore )]\nfn u() {}",
            key,
            CodeKind::Test
        ),
        ["H-12:hard@1", "H-12:hard@4"],
        "spaces before the closer"
    );
}

#[test]
fn extern_crate_alloc_and_std_are_exempt() {
    let key = "extern-crate";

    assert_eq!(
        sev_of("extern crate serde;", key, CodeKind::Prod),
        ["soft"],
        "extern crate is unnecessary"
    );
    assert_eq!(
        has(
            "#![no_std]\nextern crate alloc;\n#[cfg(test)]\nextern crate std;\nextern crate self as ledger;",
            key,
            CodeKind::Prod
        ),
        [],
        "alloc and std in a no_std crate, and self as, are exempt"
    );
}

#[test]
fn extern_crate_lookahead_backtracks_like_js() {
    let key = "extern-crate";

    assert_eq!(
        hits_of(
            "extern crate  alloc;\nextern crate alloc;\nextern crate  self as y;\nextern crate\tstd ;",
            key,
            CodeKind::Prod
        ),
        ["MOD-01:soft@1", "MOD-01:soft@3"],
        "JS gives `\\s+` back one space, and the look-ahead then sees a space"
    );
    assert_eq!(
        hits_of(
            "extern crate\nalloc;\nextern crate\n\nstd;",
            key,
            CodeKind::Prod
        ),
        ["MOD-01:soft@3"],
        "the same over newlines"
    );
}

#[test]
fn print_build_scripts_get_the_writeln_fix_text() {
    let src = "fn main() {\n    println!(\"cargo::rerun-if-changed=schema.json\");\n}";

    let bin = scan_as(src, CodeKind::Bin, FileRole::Source);
    let build = scan_as(src, CodeKind::Bin, FileRole::BuildScript);

    let (bin, build) = (hit(&bin, "print"), hit(&build, "print"));
    assert_eq!(
        (bin.severity(), build.severity()),
        (Severity::Hard, Severity::Hard),
        "print is hard in binaries and build scripts"
    );
    assert_eq!(
        build.fix(),
        "Build scripts emit directives with `writeln!(io::stdout().lock(), \"cargo::rerun-if-changed=..\")?` from a `main` that returns `Result` (ARC-17, H-06).",
        "build scripts get the 1.x fixBuild text"
    );
    assert!(
        !bin.fix().contains("cargo::"),
        "other binaries get the plain fix"
    );
    assert!(
        is_build_script("build.rs")
            && !is_build_script("src/build.rs")
            && !is_build_script("src/bin/build.rs")
            && !is_build_script("src\\bin\\build.rs"),
        "only build.rs at the crate root is a build script"
    );
}
