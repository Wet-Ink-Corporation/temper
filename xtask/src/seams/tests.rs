use super::{DOMAIN, Kit, Seam, Violation, check_file, kit_of, references, unmatched_modules};

/// The references of a file of the module `memo` (a file directly under `src/`).
fn refs(text: &str) -> Vec<(usize, String)> {
    references(text, &["memo"])
}

fn pairs(list: &[(usize, &str)]) -> Vec<(usize, String)> {
    list.iter().map(|&(l, p)| (l, p.to_owned())).collect()
}

#[test]
fn plain_paths_and_their_lines_are_found() {
    let text = "use crate::glob::Glob;\nfn f() {\n    temper_domain::js_trim(\"x\");\n}\n";
    assert_eq!(
        refs(text),
        pairs(&[(1, "crate::glob::Glob"), (3, "temper_domain::js_trim")])
    );
}

#[test]
fn use_groups_are_expanded_with_self_and_aliases() {
    let text = "use crate::{fingerprint, project::{is_js_space as space, self}, Timestamp};";
    assert_eq!(
        refs(text),
        pairs(&[
            (1, "crate::fingerprint"),
            (1, "crate::project::is_js_space"),
            (1, "crate::project"),
            (1, "crate::Timestamp"),
        ])
    );
}

#[test]
fn a_group_over_several_lines_keeps_the_first_line_and_counts_the_rest() {
    let text = "use crate::{\n    glob,\n    time,\n};\nuse crate::js;\n";
    assert_eq!(
        refs(text),
        pairs(&[(1, "crate::glob"), (1, "crate::time"), (5, "crate::js")])
    );
}

#[test]
fn comments_are_skipped() {
    let text = "// use crate::project::x;\nlet a = 1; // crate::hooks::y\n//! temper_domain::z\n\
                /* crate::a /* nested */ crate::b */ crate::glob::G;\n";
    assert_eq!(refs(text), pairs(&[(4, "crate::glob::G")]));
}

#[test]
fn other_literal_forms_are_skipped() {
    let text = "let a = '\\u{1F600}'; crate::project::a();\n\
                let b = b\"x\\\"crate::project::hidden\\\"\"; crate::project::b();\n\
                let c = r##\"x\"#crate::project::hidden\"##; crate::project::c();\n\
                let d = cr\"x\\\"; crate::project::d();\n\
                let e = cr#\"crate::project::hidden\"#; crate::project::e();\n";
    assert_eq!(
        refs(text),
        pairs(&[
            (1, "crate::project::a"),
            (2, "crate::project::b"),
            (3, "crate::project::c"),
            (4, "crate::project::d"),
            (5, "crate::project::e"),
        ])
    );
}

#[test]
fn unclosed_input_runs_to_the_end_without_a_reference() {
    for text in [
        "crate::glob::a; \"crate::project::x",
        "crate::glob::a; /* crate::project::x",
        "crate::glob::a; r#\"crate::project::x\"",
        "crate::glob::a; '\\",
    ] {
        assert_eq!(refs(text), pairs(&[(1, "crate::glob::a")]), "{text}");
    }
}

#[test]
fn self_before_super_is_read() {
    assert_eq!(
        references("use self::super::x;", &["hooks", "shell"]),
        pairs(&[(1, "crate::hooks::x")])
    );
}

#[test]
fn a_renamed_super_is_its_bare_path() {
    assert_eq!(refs("use super as up;"), pairs(&[(1, "crate")]));
    assert_eq!(
        references("use super::super;", &["hooks", "shell", "git"]),
        pairs(&[(1, "crate::hooks")])
    );
    assert_eq!(
        references("use super::super as up;", &["hooks", "shell", "git"]),
        pairs(&[(1, "crate::hooks")])
    );
}

#[test]
fn a_listed_module_with_no_file_is_reported() {
    let files = ["m.rs".to_owned(), "other.rs".to_owned()];
    assert_eq!(unmatched_modules(&SEAM, &files), vec!["hooks/shell"]);
    let files = ["m/x.rs".to_owned(), "hooks/shell/git.rs".to_owned()];
    assert_eq!(unmatched_modules(&SEAM, &files), Vec::<&str>::new());
}

#[test]
fn literals_are_skipped_and_do_not_hide_what_follows() {
    let text = "let u = \"http://x\"; crate::project::a();\n\
                let r = r#\"a//b \"crate::project::l\"\"#; crate::project::b();\n\
                let s = \"crate::project::c\"; let q = '\"'; let e = '\\''; crate::project::d();\n\
                fn f<'a>(x: &'a str) { crate::project::e(); }\n";
    assert_eq!(
        refs(text),
        pairs(&[
            (1, "crate::project::a"),
            (2, "crate::project::b"),
            (3, "crate::project::d"),
            (4, "crate::project::e"),
        ])
    );
}

#[test]
fn a_root_inside_a_longer_path_or_identifier_is_not_a_reference() {
    let text = "my_crate::a(); other::crate::b(); xtemper_domain::c(); T::<u8>::crate_x();";
    assert_eq!(refs(text), Vec::new());
}

#[test]
fn a_leading_separator_is_read() {
    assert_eq!(
        refs("use ::temper_domain::scan::Rule;"),
        pairs(&[(1, "temper_domain::scan::Rule")])
    );
}

#[test]
fn spaces_after_a_separator_are_read() {
    assert_eq!(
        refs("crate::glob::\n    Glob"),
        pairs(&[(1, "crate::glob::Glob")])
    );
}

#[test]
fn a_renamed_root_is_the_bare_root() {
    assert_eq!(
        refs("use temper_domain as d;\nuse crate as c;"),
        pairs(&[(1, "temper_domain"), (2, "crate")])
    );
}

#[test]
fn super_resolves_against_the_file_and_its_inline_modules() {
    let text = "use super::x;\nmod tests {\n    use super::*;\n    use super::super::y;\n}\n\
                use super::z;\n";
    assert_eq!(
        references(text, &["hooks", "shell"]),
        pairs(&[
            (1, "crate::hooks::x"),
            (3, "crate::hooks::shell"),
            (4, "crate::hooks::y"),
            (6, "crate::hooks::z"),
        ])
    );
}

#[test]
fn super_past_the_crate_root_stays_super() {
    assert_eq!(
        refs("use super::super::x;"),
        pairs(&[(1, "super::super::x")])
    );
}

#[test]
fn a_mod_declaration_without_a_body_opens_no_module() {
    let text = "mod tests;\nfn f() { use super::a; }\n";
    assert_eq!(
        references(text, &["glob", "parse"]),
        pairs(&[(2, "crate::glob::a")])
    );
}

#[test]
fn a_module_and_its_submodules_are_kit_bound() {
    assert_eq!(kit_of(&DOMAIN, "glob.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "glob/tests.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "state/progress.rs"), Some(Kit::Craft));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/code.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/code/lex.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/opaque.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/reviewer.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "globber.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "scan.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "glob"), None);
}

const SEAM: Seam = Seam {
    krate: "k",
    modules: &[("m", Kit::Support), ("hooks/shell", Kit::Guard)],
    crate_paths: &["Item"],
    domain_paths: &["js_trim"],
};

#[test]
fn listed_paths_pass_and_others_are_violations_with_their_line() {
    let text = "use crate::m::x;\nuse crate::hooks::shell::Words;\nuse crate::Item;\n\
                use crate::hooks::other;\nuse crate::ItemTwo;\nuse temper_domain::js_trim;\n\
                use temper_domain::scan::Rule;\nuse std::fmt;\nuse super::Item;\n";
    let found = check_file(&SEAM, "m.rs", Kit::Support, text);
    let want = [
        (4, "crate::hooks::other"),
        (5, "crate::ItemTwo"),
        (7, "temper_domain::scan::Rule"),
    ]
    .map(|(line, path)| Violation {
        file: "crates/k/src/m.rs".to_owned(),
        line,
        path: path.to_owned(),
        kit: Kit::Support,
    });
    assert_eq!(found, want);
}

#[test]
fn super_out_of_a_kit_module_is_a_violation() {
    let found = check_file(&SEAM, "m.rs", Kit::Craft, "use super::scan::Rule;");
    assert_eq!(found.len(), 1);
    let found = check_file(&SEAM, "m/n.rs", Kit::Craft, "use super::Thing;");
    assert_eq!(found, Vec::new());
}

#[test]
fn a_bare_or_renamed_root_is_a_violation() {
    let found = check_file(&SEAM, "m.rs", Kit::Craft, "use crate::{self};");
    assert_eq!(found.len(), 1);
    let found = check_file(&SEAM, "m.rs", Kit::Craft, "use temper_domain as d;");
    assert_eq!(found.len(), 1);
}

#[test]
fn a_violation_names_the_file_line_path_and_kit() {
    let v = Violation {
        file: "crates/temper-app/src/memo.rs".to_owned(),
        line: 3,
        path: "crate::project::x".to_owned(),
        kit: Kit::Craft,
    };
    assert_eq!(
        v.to_string(),
        "crates/temper-app/src/memo.rs:3: `crate::project::x` is outside the redkiln-craft seam \
         (xtask/src/seams.rs lists what it may reach)"
    );
}
