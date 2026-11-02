//! The getters `workspaceRootFor` reads, each as 1.x's `?.` chain (`manifest.mjs`).

use super::{MEMBER, ROOT, toml};

#[test]
fn package_and_workspace_are_read_with_js_truthiness() {
    let manifests = [
        ROOT,
        MEMBER,
        "workspace = false\npackage = 0\n",
        "workspace = \"\"\npackage = \"x\"\n",
        "[package]\n[workspace]\n",
    ]
    .map(toml);

    let got = manifests
        .each_ref()
        .map(|m| (m.has_package(), m.has_workspace()));

    assert_eq!(
        got,
        [
            (false, true),
            (true, false),
            (false, false),
            (true, false),
            (true, true),
        ],
        "`a.package ? ..`, `manifest?.workspace`: an empty table is truthy"
    );
}

#[test]
fn package_workspace_is_read_only_as_a_string() {
    let manifests = [
        "[package]\nworkspace = \"../..\"\n",
        "[package]\nworkspace = 1\n",
        "[package]\n",
        "package = \"workspace\"\n",
    ]
    .map(toml);

    let got = manifests.each_ref().map(|m| m.package_workspace());

    assert_eq!(
        got,
        [Some("../.."), None, None, None],
        "`typeof explicit === 'string'`"
    );
}

/// `(m.workspace.exclude ?? [])` with `String(e)` per entry; OC-3 (q): an exclude that is not an
/// array reads as empty where 1.x's `.some` would throw.
#[test]
fn workspace_excludes_are_js_strings_and_a_non_array_reads_as_empty() {
    let manifests = [
        ROOT,
        "[workspace]\nexclude = [\"a\", 1, true, [2, 3], { b = 1 }]\n",
        "[workspace]\nexclude = \"x\"\n",
        "[workspace]\n",
        "workspace = true\n",
        MEMBER,
    ]
    .map(toml);

    let got = manifests.each_ref().map(|m| {
        m.workspace_excludes()
            .map(std::borrow::Cow::into_owned)
            .collect::<Vec<_>>()
    });

    assert_eq!(
        got,
        [
            vec!["crates/scratch".to_owned(), "./tools/gen/".to_owned()],
            ["a", "1", "true", "2,3", "[object Object]"]
                .map(str::to_owned)
                .to_vec(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ],
        "String(e) per entry"
    );
}
