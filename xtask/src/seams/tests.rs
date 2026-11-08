use super::{DOMAIN, Kit, Seam, Violation, check_file, is_test_file, kit_of, references};

fn refs(text: &str) -> Vec<(usize, String)> {
    references(text)
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
    let text = "// use crate::project::x;\nlet a = 1; // crate::hooks::y\n//! temper_domain::z\n";
    assert_eq!(refs(text), Vec::new());
}

#[test]
fn a_root_inside_a_longer_path_or_identifier_is_not_a_reference() {
    let text = "my_crate::a(); other::crate::b(); xtemper_domain::c();";
    assert_eq!(refs(text), Vec::new());
}

#[test]
fn spaces_after_a_separator_are_read() {
    assert_eq!(
        refs("crate::glob::\n    Glob"),
        pairs(&[(1, "crate::glob::Glob")])
    );
}

#[test]
fn test_files_are_tests_rs_or_under_a_tests_directory() {
    assert!(is_test_file("glob/tests.rs"));
    assert!(is_test_file("tests.rs"));
    assert!(is_test_file("hooks/shell/tests/fixtures.rs"));
    assert!(!is_test_file("hooks/shell/words.rs"));
    assert!(!is_test_file("tests_helper.rs"));
    assert!(!is_test_file("glob/tests"));
}

#[test]
fn a_module_and_its_submodules_are_kit_bound() {
    assert_eq!(kit_of(&DOMAIN, "glob.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "state/progress.rs"), Some(Kit::Craft));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/code.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash/code/lex.rs"), Some(Kit::Guard));
    assert_eq!(kit_of(&DOMAIN, "hooks/bash.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "globber.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "scan.rs"), None);
    assert_eq!(kit_of(&DOMAIN, "glob"), None);
}

const SEAM: Seam = Seam {
    krate: "k",
    modules: &[("m", Kit::Support)],
    crate_paths: &["m", "hooks::shell", "Item"],
    domain_paths: &["js_trim"],
};

#[test]
fn listed_paths_pass_and_others_are_violations_with_their_line() {
    let text = "use crate::m::x;\nuse crate::hooks::shell::Words;\nuse crate::Item;\n\
                use crate::hooks::other;\nuse crate::ItemTwo;\nuse temper_domain::js_trim;\n\
                use temper_domain::scan::Rule;\nuse std::fmt;\n";
    let found = check_file(&SEAM, "crates/k/src/m.rs", Kit::Support, text);
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
fn a_bare_root_is_a_violation() {
    let found = check_file(&SEAM, "f.rs", Kit::Craft, "use crate::{self};");
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
