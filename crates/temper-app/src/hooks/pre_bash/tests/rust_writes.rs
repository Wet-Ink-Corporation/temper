//! `tests/hooks.test.mjs`'s Bash tests of the copy, patch and reviewer branches, one test
//! each, in its order. `rust_writes_cases.json` holds every `preBash` call those tests make,
//! with the answer 1.x gave and the files under the temporary directories at that moment
//! (captured by running the 1.x test file with `preBash` wrapped; the example workspace's
//! vendored CLI and CI files, which no Bash check reads, are left out). A long command or
//! answer is stored as runs of a repeated piece. The wall-clock bounds of the four timing
//! tests are retired into the hook parity harness's per-step budget (OC-4 A, wi-3ea323);
//! their decisions are kept. git answers as outside a repository, as it did in the capture.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use temper_domain::{HookEvent, HookOutcome, JsValue, PathStyle::Posix, Scanner};

use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::hooks::HookInput;
use crate::ports::{EntryKind, ExitKind, Files};

const CASES: &str = include_str!("rust_writes_cases.json");

/// Item `i` of an array.
fn at(v: &JsValue, i: usize) -> Option<&JsValue> {
    match v {
        JsValue::Array(items) => items.get(i),
        _ => None,
    }
}

/// A count or an id, a JSON integer.
fn count(v: Option<&JsValue>) -> usize {
    v.map(JsValue::to_js_string)
        .and_then(|n| n.parse().ok())
        .expect("a non-negative integer")
}

/// A field of a case, as a string: a JSON string, or `{"rle": [[piece, times], ...]}`.
fn text(case: &JsValue, key: &str) -> Option<String> {
    match case.get(key)? {
        JsValue::String(s) => Some(s.clone()), // the case keeps its own
        rle => {
            let Some(JsValue::Array(runs)) = rle.get("rle") else {
                panic!("{key} is a string or runs");
            };
            let mut out = String::new();
            for run in runs {
                let piece = at(run, 0).and_then(JsValue::as_str).expect("a piece");
                out.push_str(&piece.repeat(count(at(run, 1))));
            }
            Some(out)
        }
    }
}

/// The files of snapshot `snap`: files with their text, empty directories, links, and a
/// listing for each directory that holds a link, which the fake would not list otherwise.
fn snapshot(all: &JsValue, snap: usize) -> FakeFiles {
    let texts = all.get("texts").expect("texts");
    let Some(JsValue::Array(entries)) = all.get("snaps").and_then(|s| at(s, snap)) else {
        panic!("snapshot {snap}");
    };
    let mut files = FakeFiles::new(Posix);
    let mut dirs = Vec::new();
    let mut children: BTreeMap<String, BTreeMap<String, EntryKind>> = BTreeMap::new();
    let mut linked = BTreeSet::new();
    for e in entries {
        let kind = at(e, 0).and_then(JsValue::as_str).expect("a kind");
        let path = at(e, 1).and_then(JsValue::as_str).expect("a path");
        let (parent, name) = path.rsplit_once('/').expect("an absolute path");
        let entry = match kind {
            "file" => {
                let body = at(texts, count(at(e, 2)))
                    .and_then(JsValue::as_str)
                    .expect("a text");
                files = files.with_file(path, body);
                EntryKind::File
            }
            "dir" => {
                dirs.push(path.to_owned());
                EntryKind::Dir
            }
            "link" => {
                let to = at(e, 2).and_then(JsValue::as_str).expect("a target");
                files = files.with_link(path, to);
                linked.insert(parent.to_owned());
                EntryKind::Symlink
            }
            _ => EntryKind::Other,
        };
        children
            .entry(parent.to_owned())
            .or_default()
            .insert(name.to_owned(), entry);
        // Each ancestor lists the directory below it.
        let mut child = parent;
        while let Some((up, name)) = child.rsplit_once('/').filter(|(up, _)| !up.is_empty()) {
            children
                .entry(up.to_owned())
                .or_default()
                .insert(name.to_owned(), EntryKind::Dir);
            child = up;
        }
    }
    for dir in &linked {
        let entries: Vec<(&str, EntryKind)> = children[dir]
            .iter()
            .map(|(name, &kind)| (name.as_str(), kind))
            .collect();
        files = files.with_listing(dir, &entries);
    }
    for dir in dirs {
        files
            .create_dir_all(Path::new(&dir))
            .expect("the fake makes it");
    }
    files
}

/// The answer as the capture writes it.
fn answer(outcome: &HookOutcome) -> (&'static str, Option<&str>, Option<&str>) {
    fn some(s: &Option<String>) -> Option<&str> {
        s.as_deref().filter(|s| !s.is_empty())
    }
    match outcome {
        HookOutcome::Pass => ("none", None, None),
        HookOutcome::Ask { reason, context } => ("ask", Some(reason), some(context)),
        HookOutcome::Deny { reason, context } => ("deny", Some(reason), some(context)),
        HookOutcome::Context { text, .. } => ("context", None, Some(text)),
        HookOutcome::Notice { message } => ("notice", Some(message), None),
    }
}

/// Every captured call of 1.x's test `test` gets 1.x's answer.
fn replays(test: &str) {
    let all = JsValue::from_json(CASES).expect("the cases are JSON");
    let Some(JsValue::Array(groups)) = all.get("groups") else {
        panic!("the cases have groups");
    };
    let group = groups
        .iter()
        .find(|g| g.get("test").and_then(JsValue::as_str) == Some(test))
        .expect("the test has captured cases");
    let Some(JsValue::Array(cases)) = group.get("cases") else {
        panic!("{test} has cases");
    };
    let scanner = Scanner::new().expect("every built-in pattern compiles");
    let mut ports: BTreeMap<usize, FakePorts> = BTreeMap::new();
    let mut wrong = Vec::new();
    for case in cases {
        let snap = count(case.get("snap"));
        let ports = ports.entry(snap).or_insert_with(|| {
            let mut ports = FakePorts::new(snapshot(&all, snap));
            ports.process = FakeProcess::new().otherwise(Reply::Exit {
                status: ExitKind::Code(128),
                stdout: String::new(),
            });
            ports
        });
        let cmd = text(case, "cmd").expect("a command");
        let cwd = text(case, "cwd");
        let mut input = format!(
            r#"{{"tool_name":"Bash","session_id":"hooks-test","tool_input":{{"command":{}}}"#,
            JsValue::String(cmd.clone()).to_json() // the input's own copy
        );
        if let Some(cwd) = &cwd {
            input.push_str(&format!(
                r#","cwd":{}"#,
                JsValue::String(cwd.clone()).to_json()
            ));
        }
        if let Some(agent) = text(case, "agent") {
            input.push_str(&format!(
                r#","agent_type":{}"#,
                JsValue::String(agent).to_json()
            ));
        }
        input.push('}');
        let env = RunEnv {
            cwd: PathBuf::from("/"),
            style: Posix,
            plugin_data: None,
            temp_dir: PathBuf::from("/tmp"),
            home: PathBuf::from(text(case, "home").unwrap_or_default()),
            remote_session: None,
        };
        let input = HookInput::parse(&input).expect("the input is JSON");
        let call = crate::hooks::HookCall {
            ports: &*ports,
            env: &env,
            input: &input,
        };
        let got = crate::hooks::run(&call, &scanner, HookEvent::PreBash).expect("an answer");
        let reason = text(case, "reason");
        let context = text(case, "context");
        let want = (
            text(case, "decision").unwrap_or_default(),
            reason.as_deref(),
            context.as_deref(),
        );
        let (decision, reason, context) = answer(&got);
        if (decision, reason, context) != (want.0.as_str(), want.1, want.2) {
            let short: String = cmd.chars().take(160).collect();
            wrong.push(format!(
                "{short:?}: {decision} {reason:?} {context:?}\n   1.x: {want:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{test}: {} of {} answer differently from 1.x:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

#[test]
fn a_copy_onto_rs_is_scanned_like_a_write() {
    replays(
        "Bash: cp/mv/install/Copy-Item onto a .rs file is scanned like a Write: new escape hatches deny, #[expect] and test removal ask",
    );
}

#[test]
fn a_copy_onto_rs_whose_content_temper_cannot_read_is_refused() {
    replays(
        "Bash: a copy onto .rs whose content temper cannot read is refused with the Edit/Write guidance",
    );
}

#[test]
fn git_apply_and_patch_that_touch_rs_files_are_refused() {
    replays(
        "Bash: git apply and patch that touch .rs files are refused; read-only forms and non-Rust patches are not",
    );
}

#[test]
fn a_denied_rust_patch_names_the_scanned_routes() {
    replays("Bash: a denied Rust patch names the scanned routes and says a retry is denied again");
}

#[test]
fn the_read_only_reviewer_may_not_apply_any_patch() {
    replays("Bash: the read-only reviewer may not apply any patch, and is told not to retry");
}

#[test]
fn every_command_temper_tells_the_reviewer_to_run_is_not_denied() {
    replays("Bash: every command temper tells the reviewer to run is not denied");
}

#[test]
fn interpreter_code_with_a_megabyte_of_spaces_or_escapes_is_decided() {
    replays(
        "Bash: interpreter code with a megabyte of spaces or 50,000 escapes after a call is decided in under a second",
    );
}

#[test]
fn a_megabyte_of_spaces_parentheses_or_call_openers_is_decided() {
    replays("Bash: a megabyte of spaces, parentheses or call openers is decided in under a second");
}

#[test]
fn a_long_run_of_shell_writer_words_or_equals_is_decided() {
    replays("Bash: a long run of shell-writer words or `=` is decided in under a second");
}

#[test]
fn a_48_kb_printf_piped_into_a_shell_is_decided() {
    replays("Bash: a 48 KB printf command piped into a shell is decided in well under a second");
}

#[test]
fn gpatch_busybox_patch_watch_and_git_c_aliases_touching_rs_are_refused() {
    replays(
        "Bash: gpatch, busybox patch, watch and git -c aliases that touch .rs files are refused for any agent (#151)",
    );
}

#[test]
fn renames_clean_copies_copies_out_and_non_rust_copies_stay_allowed() {
    replays(
        "Bash: renames, copies that pass the scan, copies out of the workspace and non-Rust copies stay allowed",
    );
}

#[test]
fn cd_subshells_and_pushd_popd_resolve_the_directory_a_rename_runs_in() {
    replays("Bash: cd, subshells and pushd/popd resolve the directory a rename runs in");
}

#[test]
fn copies_into_the_scratch_directories_are_not_scanned() {
    replays(
        "Bash: copies into the gate-excluded scratch directories (.temper/, target/) are not scanned",
    );
}

#[test]
fn a_heredoc_patch_is_read_and_a_patch_made_earlier_is_named() {
    replays(
        "Bash: a heredoc patch is read from the command; a patch file made earlier in the command is named as such",
    );
}

#[test]
fn diff_u_headers_with_timestamps_and_crlf_patches_touch_rs_files() {
    replays(
        "Bash: diff -u headers with timestamps and CRLF patches are recognised as touching .rs files",
    );
}

#[test]
fn copies_inside_compound_commands_and_wrappers_are_scanned() {
    replays(
        "Bash: copies inside if/for/while/brace groups and wrapped commands are scanned; everyday wrapped commands are not",
    );
}

#[test]
fn a_move_into_production_code_and_a_laundered_copy_are_scanned() {
    replays(
        "Bash: a move that turns test or example code into production code is scanned, and so is a copy laundered through a temp path",
    );
}

#[test]
fn a_crate_moved_or_copied_whole_keeps_its_layout() {
    replays("Bash: a crate moved or copied whole keeps its layout, so its unit tests stay tests");
}

#[test]
fn patch_help_and_version_apply_nothing_and_git_am_and_rsync_are_read() {
    replays(
        "Bash: patch and git apply/am --help and --version apply nothing; git am and rsync are read like git apply and cp",
    );
}

#[test]
fn a_home_source_is_read_from_the_home_directory_and_scanned() {
    replays("Bash: a ~/ source is read from the home directory and scanned");
}

#[test]
fn a_backup_to_a_temp_dir_or_variable_passes_and_unclean_content_is_refused() {
    replays(
        "Bash: a .rs file backed up to a temp dir or a variable destination passes; unclean content to an unknown place is still refused",
    );
}

#[test]
fn escaped_whatif_linked_and_laundered_rust_copies_are_scanned() {
    replays(
        "Bash: Rust copies spelled c\\p, run with -WhatIf:$false, linked with ln or laundered through an existing file are scanned",
    );
}

#[test]
fn a_symlinked_rs_file_in_a_copied_tree_is_scanned_for_its_target() {
    replays(
        "Bash: a symlinked .rs file inside a recursively copied tree is scanned for what it points at",
    );
}

#[test]
fn a_patch_quoting_a_path_with_spaces_touches_an_rs_file() {
    replays(
        "Bash: a patch whose git headers quote a path with spaces is still read as touching a .rs file",
    );
}

#[test]
fn a_copys_clone_advisory_survives_another_ask() {
    replays("Bash: a copy's H-08 advisory survives when another check in the command asks");
}

#[test]
fn a_copy_whose_source_an_earlier_write_rewrites_is_refused() {
    replays(
        "Bash: a Rust copy whose source an earlier non-copy write in the command rewrites is refused; later or unrelated writes are not",
    );
}

#[test]
fn ln_sr_reads_its_target_from_the_current_directory() {
    replays(
        "Bash: ln -sr reads its target from the current directory, and a linked directory of .rs files is scanned",
    );
}

#[test]
fn a_shell_write_into_rs_spelled_with_escapes_or_by_curl_or_dd_is_denied() {
    replays(
        "Bash: a shell write into a .rs file spelled with a backslash or split quotes, or by curl or dd, is denied like a redirect",
    );
}
