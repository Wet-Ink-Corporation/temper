use super::{MEMBER, ROOT, no_lints};
use crate::manifest::{lint_config_changed, lint_sections};

#[test]
fn lint_config_changed_ignores_plain_inheritance_and_comments() {
    assert!(
        !lint_config_changed(&no_lints(), MEMBER),
        "gaining `[lints] workspace = true`"
    );
    assert!(
        !lint_config_changed("", MEMBER),
        "a new member that inherits"
    );
    let commented = ROOT.replace(
        r#"unsafe_code = "forbid""#,
        r#"unsafe_code = "forbid" # H-01"#,
    );
    assert!(!lint_config_changed(ROOT, &commented), "a comment");
    assert!(
        lint_config_changed(ROOT, &ROOT.replace(r#"lto = "thin""#, r#"lto = "fat""#)),
        "a profile setting"
    );
    let overriding = MEMBER.replace(
        "workspace = true\n\n[dep",
        "workspace = true\n\n[lints.rust]\nunsafe_code = \"allow\"\n\n[dep",
    );
    assert!(
        lint_config_changed(MEMBER, &overriding),
        "a member lint beside the inheritance"
    );
}

#[test]
fn invalid_toml_compares_the_section_lines() {
    let broken = "[package\n[lints]\nworkspace = true\n";
    // 1.x's `lintConfigChanged` on the same texts: blank lines drop out, a comment in a
    // lint section counts.
    assert!(
        !lint_config_changed(broken, &format!("{broken}\n\n")),
        "only blank lines"
    );
    assert!(
        lint_config_changed(broken, &format!("{broken}# note\n")),
        "a comment in [lints]"
    );
    assert!(
        lint_config_changed(broken, "[package\n[lints]\nworkspace = false\n"),
        "a lint line differs"
    );
}

#[test]
fn lint_sections_keeps_lint_and_profile_lines() {
    let text = "[package]\nname = \"a\"\n\n[workspace.lints.rust]\n  unsafe_code = \"forbid\"  \r\n\n[profile.release]\nlto = true\n[[bin]]\nname = \"x\"\n[profiles]\nz = 1\n[dependencies]\nlints = 1\n";
    assert_eq!(
        lint_sections(text),
        "[workspace.lints.rust]\nunsafe_code = \"forbid\"\n[profile.release]\nlto = true\n[[bin]]\nname = \"x\""
    );
}
