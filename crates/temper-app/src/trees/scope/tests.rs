//! The scopes of the session's checkouts, over the fakes. Real git runs in `temper-cli`'s
//! `subagent_start` tests and the parity cases.

use std::borrow::Cow;

use temper_domain::PathStyle::Posix;
use temper_domain::{Config, tree_id};

use super::{Checkout, Scope, Tree, Widen, covers_base, other_trees, own_tree};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::hooks::{HookCall, HookInput};
use crate::ports::ExitKind;
use crate::project::Workspace;

const MAIN: &str = "/r";
const STATE: &str = "/r/.git/temper";
const SESSION: &str = "/r/.git/temper/sessions/s1";
const H1: &str = "1111111111111111111111111111111111111111";
const H2: &str = "2222222222222222222222222222222222222222";
const H3: &str = "3333333333333333333333333333333333333333";

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn failed() -> Reply {
    Reply::Exit {
        status: ExitKind::Code(1),
        stdout: String::new(),
    }
}

fn workspace(root: &str) -> Workspace {
    Workspace {
        root: root.to_owned(),
        state_dir: STATE.to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: true,
    }
}

/// `process` answering as a checkout at `root` of the repository at [`MAIN`]: its top level,
/// its common dir, and its git dirs (linked unless it is [`MAIN`]).
fn checkout(process: FakeProcess, root: &str) -> FakeProcess {
    let git_dir = if root == MAIN {
        "/r/.git".to_owned()
    } else {
        format!(
            "/r/.git/worktrees/{}",
            root.rsplit('/').next().unwrap_or("")
        )
    };
    process
        .answering(
            &["git", "rev-parse", "--show-toplevel"],
            root,
            ran(&format!("{root}\n")),
        )
        .answering(
            &["git", "rev-parse", "--git-common-dir"],
            root,
            ran("/r/.git\n"),
        )
        .answering(
            &["git", "rev-parse", "--git-dir", "--git-common-dir"],
            root,
            ran(&format!("{git_dir}\n/r/.git\n")),
        )
}

/// `rev` resolves to `sha` at `root`.
fn resolving(process: FakeProcess, root: &str, rev: &str, sha: &str) -> FakeProcess {
    let commit = format!("{rev}^{{commit}}");
    process.answering(
        &["git", "rev-parse", "--verify", "--quiet", &commit],
        root,
        ran(&format!("{sha}\n")),
    )
}

fn record(root: &str, head: &str, at: u32) -> String {
    format!(r#"{{"root":"{root}","head":"{head}","branch":null,"startFp":"fp-{at}","at":{at}}}"#)
}

fn record_path(root: &str, agent: &str) -> String {
    format!("{SESSION}/tree-{}-{agent}.json", tree_id(root))
}

fn with_checkout(files: FakeFiles, root: &str) -> FakeFiles {
    files.with_file(&format!("{root}/Cargo.toml"), "[workspace]\n")
}

fn call_with<T>(
    ports: &FakePorts,
    cwd: &str,
    input: &str,
    f: impl FnOnce(&HookCall<'_, FakePorts>) -> T,
) -> T {
    let input = HookInput::parse(input).expect("JSON");
    let env = RunEnv::at(cwd, Posix);
    let call = HookCall {
        ports,
        env: &env,
        input: &input,
    };
    f(&call)
}

const SID: &str = r#"{"session_id":"s1"}"#;

#[test]
fn other_trees_keep_each_root_s_earliest_record_in_byte_order() {
    let files = [MAIN, "/r/wt-a", "/r/wt-b", "/r/wt-c"]
        .into_iter()
        .fold(FakeFiles::new(Posix), with_checkout)
        .with_file(
            &record_path("/r/wt-b", "agent-b1"),
            &record("/r/wt-b", "late", 5),
        )
        .with_file(
            &record_path("/r/wt-b", "agent-b2"),
            &record("/r/wt-b", "early", 3),
        )
        .with_file(
            &record_path("/r/wt-a", "agent-a1"),
            &record("/r/wt-a", "a", 9),
        )
        .with_file(&record_path(MAIN, "main"), &record(MAIN, "m", 1))
        .with_file(
            &record_path("/r/gone", "agent-g1"),
            &record("/r/gone", "g", 1),
        )
        .with_file(
            &format!("{SESSION}/tree-0123456789ab-.json"),
            &record("/r/wt-c", "c", 1),
        )
        .with_file(
            &format!("{SESSION}/tree-0123456789AB-x.json"),
            &record("/r/wt-c", "c", 1),
        )
        .with_file(&format!("{SESSION}/tree-0123456789ab-x.json"), "not json");
    let process = ["/r/wt-a", "/r/wt-b"]
        .into_iter()
        .fold(checkout(FakeProcess::new(), MAIN), checkout);
    let process = resolving(process, "/r/wt-b", "early", H2);
    let process = resolving(process, "/r/wt-a", "a", H1).otherwise(failed());
    let ports = FakePorts {
        process,
        ..FakePorts::new(files)
    };

    let trees = call_with(&ports, MAIN, SID, |call| {
        other_trees(call, &workspace(MAIN))
    })
    .expect("no throw");

    let got: Vec<(&str, &Scope)> = trees
        .iter()
        .map(|t| (t.ws.root.as_str(), &t.scope))
        .collect();
    let scope = |base: &str, fp: &str| Scope {
        base: Some(base.to_owned()),
        start_fp: Some(fp.to_owned()),
    };
    assert_eq!(
        got,
        vec![
            ("/r/wt-a", &scope(H1, "fp-9")),
            ("/r/wt-b", &scope(H2, "fp-3"))
        ],
        "this checkout, a missing root and bad names are left out; wt-b's earliest record counts"
    );
    assert!(trees.iter().all(|t| t.checkout == Checkout::Other));
}

#[test]
fn a_root_that_is_another_project_or_state_dir_is_left_out() {
    let files = with_checkout(with_checkout(FakeFiles::new(Posix), MAIN), "/r/wt-a/sub").with_file(
        &record_path("/r/wt-a/sub", "agent-a1"),
        &record("/r/wt-a/sub", "a", 1),
    );
    // The record's root is inside a checkout whose top level is above it.
    let process = FakeProcess::new()
        .answering(
            &["git", "rev-parse", "--show-toplevel"],
            "/r/wt-a/sub",
            ran("/r/wt-a\n"),
        )
        .otherwise(failed());
    let ports = FakePorts {
        process,
        ..FakePorts::new(files)
    };

    let trees = call_with(&ports, MAIN, SID, |call| {
        other_trees(call, &workspace(MAIN))
    });

    assert_eq!(trees.map(|t| t.len()), Ok(0));
}

#[test]
fn a_record_whose_root_is_not_a_string_fails_as_path_resolve_throws() {
    let files =
        FakeFiles::new(Posix).with_file(&record_path("/x", "agent-a1"), r#"{"root":5,"at":1}"#);
    let ports = FakePorts::new(files);

    let trees = call_with(&ports, MAIN, SID, |call| {
        other_trees(call, &workspace(MAIN))
    });

    assert!(
        matches!(trees, Err(error) if error.message().contains("root")),
        "F5"
    );
}

#[test]
fn without_a_session_id_or_a_session_dir_there_are_no_other_trees() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    for input in ["{}", SID] {
        let trees = call_with(&ports, MAIN, input, |call| {
            other_trees(call, &workspace(MAIN))
        });
        assert_eq!(trees.map(|t| t.len()), Ok(0), "{input}");
    }
}

const WT: &str = "/r/wt";

/// A worktree [`WT`] whose session base is `B` (at [`H1`]), with HEAD at [`H3`].
fn worktree_ports(files: FakeFiles, ancestor: bool) -> FakePorts {
    let process = checkout(FakeProcess::new(), WT);
    let process = resolving(process, WT, "B", H1);
    let process = resolving(process, WT, "r2", H2);
    let is_ancestor = if ancestor { ran("") } else { failed() };
    let process = process
        .answering(
            &["git", "merge-base", "--is-ancestor", H1, H2],
            WT,
            is_ancestor,
        )
        .otherwise(failed());
    FakePorts {
        process,
        ..FakePorts::new(
            files
                .with_file(&format!("{SESSION}/base"), "B")
                .with_file(&format!("{SESSION}/start-fp"), "sfp\n"),
        )
    }
}

fn own(ports: &FakePorts, input: &str, widen: Widen) -> Scope {
    call_with(ports, WT, input, |call| {
        own_tree(call, &workspace(WT), widen)
    })
    .expect("no throw")
}

fn scope(base: &str, start_fp: Option<&str>) -> Scope {
    Scope {
        base: Some(base.to_owned()),
        start_fp: start_fp.map(str::to_owned),
    }
}

#[test]
fn own_tree_is_the_earliest_record_with_a_head_or_the_session_s() {
    let files = FakeFiles::new(Posix)
        .with_file(&record_path(WT, "agent-a1"), &record(WT, "r2", 4))
        .with_file(&record_path(WT, "agent-a2"), r#"{"root":"/r/wt","at":1}"#)
        .with_file(&record_path(WT, "agent-a3"), &record(WT, "r9", 7));
    let ports = worktree_ports(files, false);

    assert_eq!(
        own(&ports, SID, Widen::No),
        scope(H2, Some("fp-4")),
        "no head: skipped"
    );
    assert_eq!(
        own(&ports, SID, Widen::ToSessionBase),
        scope(H2, Some("fp-4")),
        "the session base is not an ancestor"
    );
    let unknown = Scope {
        base: None,
        start_fp: None,
    };
    assert_eq!(
        own(&ports, "{}", Widen::No),
        unknown,
        "no session id: the `unknown` session's"
    );
    let empty = worktree_ports(FakeFiles::new(Posix), true);
    assert_eq!(
        own(&empty, SID, Widen::No),
        scope(H1, Some("sfp")),
        "no record"
    );
}

#[test]
fn the_main_agent_s_scope_widens_to_an_ancestor_session_base() {
    let files = FakeFiles::new(Posix).with_file(&record_path(WT, "agent-a1"), &record(WT, "r2", 4));
    let ports = worktree_ports(files, true);

    assert_eq!(own(&ports, SID, Widen::ToSessionBase), scope(H1, None));
    assert_eq!(own(&ports, SID, Widen::No), scope(H2, Some("fp-4")));
}

#[test]
fn times_js_cannot_compare_give_the_session_s_scope_as_1x_s_try_does() {
    let files = FakeFiles::new(Posix)
        .with_file(&record_path(WT, "agent-a1"), &record(WT, "r2", 4))
        .with_file(
            &record_path(WT, "agent-a2"),
            r#"{"root":"/r/wt","head":"r2","at":{"toString":1}}"#,
        );
    let ports = worktree_ports(files, false);

    assert_eq!(own(&ports, SID, Widen::No), scope(H1, Some("sfp")));
}

#[test]
fn covers_base_counts_the_tree_s_own_commits_and_a_failed_count_is_not_covered() {
    let main = workspace(MAIN);
    let count = |n: &str| ran(&format!("{n}\n"));
    let ports = |all: Reply, not_base: Reply| {
        resolving(FakeProcess::new(), WT, "HEAD", H3)
            .answering(&["git", "rev-list", "--count", H3, "^T"], MAIN, all)
            .answering(
                &["git", "rev-list", "--count", H3, "^T", "^B"],
                MAIN,
                not_base,
            )
            .otherwise(failed())
    };
    let covers = |process: &FakeProcess, base: Option<&str>, tree_base: Option<&str>| {
        let tree = Tree {
            ws: Cow::Owned(workspace(WT)),
            scope: Scope {
                base: tree_base.map(str::to_owned),
                start_fp: None,
            },
            checkout: Checkout::Other,
        };
        covers_base(process, &main, base, &tree)
    };

    assert!(
        covers(&ports(count("2"), count("2")), Some("B"), Some("T")),
        "none of them in B"
    );
    assert!(
        !covers(&ports(count("2"), count("1")), Some("B"), Some("T")),
        "one is in B"
    );
    assert!(
        !covers(&ports(failed(), failed()), Some("B"), Some("T")),
        "#157"
    );
    assert!(
        !covers(&ports(count("x"), count("x")), Some("B"), Some("T")),
        "NaN"
    );
    assert!(
        covers(&ports(failed(), failed()), Some("B"), None),
        "no tree base"
    );
    assert!(
        !covers(&ports(count("2"), count("2")), None, Some("T")),
        "no base"
    );
    let headless = FakeProcess::new().otherwise(failed());
    assert!(!covers(&headless, Some("B"), Some("T")), "no HEAD");
}
