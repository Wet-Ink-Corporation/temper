use temper_domain::PathStyle::Posix;
use temper_domain::{Config, JsValue, tree_id};

use super::note_session_tree;
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::hooks::{HookCall, HookInput};
use crate::ports::{ExitKind, FsOp};
use crate::project::Workspace;
use crate::session::Force;

const ROOT: &str = "/r/.claude/worktrees/wt";
const HEAD: &str = "1111111111111111111111111111111111111111";
const SESSION: &str = "/r/.git/temper/sessions/s1";

fn workspace(git: bool) -> Workspace {
    Workspace {
        root: ROOT.to_owned(),
        state_dir: "/r/.git/temper".to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git,
    }
}

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn failed() -> Reply {
    Reply::Exit {
        status: ExitKind::Code(128),
        stdout: String::new(),
    }
}

/// A checkout at [`ROOT`] whose git dirs are `dirs`, with HEAD at [`HEAD`] when `head`.
fn ports(files: FakeFiles, dirs: &str, head: bool) -> FakePorts {
    let rev_parse = if head { ran(&format!("{HEAD}\n")) } else { failed() };
    FakePorts {
        process: FakeProcess::new()
            .answering(
                &["git", "rev-parse", "--git-dir", "--git-common-dir"],
                ROOT,
                ran(dirs),
            )
            .answering(&["git", "rev-parse", "HEAD"], ROOT, rev_parse)
            .otherwise(failed()),
        ..FakePorts::new(files)
    }
}

const LINKED: &str = "/r/.git/worktrees/wt\n/r/.git\n";

fn record_path() -> String {
    format!("{SESSION}/tree-{}-session.json", tree_id(ROOT))
}

fn note(ports: &FakePorts, ws: &Workspace, input: &str, force: Force) -> Result<(), FsOp> {
    let input = HookInput::parse(input).expect("JSON");
    let env = RunEnv::at(ROOT, Posix);
    let call = HookCall {
        ports,
        env: &env,
        input: &input,
    };
    note_session_tree(&call, ws, force).map_err(|error| error.op())
}

const SID: &str = r#"{"session_id":"s1"}"#;

#[test]
fn a_linked_worktree_records_the_session_base_and_start_fingerprint() {
    let files = FakeFiles::new(Posix).with_file(&format!("{SESSION}/start-fp"), "abcd\n");
    let ports = ports(files, LINKED, true);

    note(&ports, &workspace(true), SID, Force::Always).expect("written");

    let record = ports.files.text(&record_path()).expect("the record");
    assert_eq!(
        record,
        format!(
            r#"{{"root":"{ROOT}","head":"{HEAD}","branch":null,"startFp":"abcd","at":0}}"#
        ),
        "1.x's JSON.stringify of the record"
    );
}

#[test]
fn without_a_start_fingerprint_the_record_says_null() {
    let ports = ports(FakeFiles::new(Posix), LINKED, true);

    note(&ports, &workspace(true), SID, Force::Always).expect("written");

    let record = JsValue::from_json(&ports.files.text(&record_path()).expect("written"))
        .expect("JSON");
    assert_eq!(record.get("startFp"), Some(&JsValue::Null), "startFingerprint is null");
}

#[test]
fn only_a_linked_worktree_with_a_session_and_a_base_is_recorded() {
    let main = "/r/.git\n/r/.git\n";
    for (dirs, git, input, head, why) in [
        (main, true, SID, true, "the main checkout"),
        (LINKED, false, SID, true, "no git root"),
        (LINKED, true, r#"{"session_id":""}"#, true, "no session"),
        (LINKED, true, "{}", true, "no session id"),
        (LINKED, true, SID, false, "no base"),
        ("/r/.git/worktrees/wt\n", true, SID, true, "one line only"),
    ] {
        let ports = ports(FakeFiles::new(Posix), dirs, head);
        note(&ports, &workspace(git), input, Force::Always).expect("nothing to write");
        assert_eq!(ports.files.paths(), Vec::<String>::new(), "{why}");
    }
}

#[test]
fn git_dirs_that_fail_read_as_not_linked() {
    let ports = FakePorts {
        process: FakeProcess::new().otherwise(failed()),
        ..FakePorts::new(FakeFiles::new(Posix))
    };

    note(&ports, &workspace(true), SID, Force::Always).expect("nothing to write");

    assert_eq!(ports.files.paths(), Vec::<String>::new(), "isLinked is false");
}

#[test]
fn a_resume_writes_only_when_no_record_names_this_checkout() {
    let other = format!("{SESSION}/tree-{}-agent-a1.json", tree_id(ROOT));
    let elsewhere = format!("{SESSION}/tree-000000000000-session.json");

    let ports_with = ports(FakeFiles::new(Posix).with_file(&other, "{}"), LINKED, true);
    note(&ports_with, &workspace(true), SID, Force::IfAbsent).expect("nothing to write");
    let ports_without = ports(FakeFiles::new(Posix).with_file(&elsewhere, "{}"), LINKED, true);
    note(&ports_without, &workspace(true), SID, Force::IfAbsent).expect("written");

    assert_eq!(ports_with.files.text(&record_path()), None, "an agent's record came first");
    assert!(
        ports_without.files.text(&record_path()).is_some(),
        "another checkout's record does not count"
    );
}

#[test]
fn a_fresh_start_always_rewrites_the_record() {
    let files = FakeFiles::new(Posix).with_file(&record_path(), "old");
    let ports = ports(files, LINKED, true);

    note(&ports, &workspace(true), SID, Force::Always).expect("written");

    assert_ne!(ports.files.text(&record_path()).as_deref(), Some("old"), "rewritten");
}

#[test]
fn a_write_failure_is_the_handler_s_error() {
    let files = FakeFiles::new(Posix).with_read_only(SESSION);
    let ports = ports(files, LINKED, true);

    let error = note(&ports, &workspace(true), SID, Force::Always);

    assert_eq!(error, Err(FsOp::Write), "1.x's writeAtomic throws (F1)");
}
