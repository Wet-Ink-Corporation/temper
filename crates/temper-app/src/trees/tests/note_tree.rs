//! `note_tree` (1.x `noteTree`): the record of a checkout an agent context changes Rust in.

use temper_domain::PathStyle::Posix;
use temper_domain::{JsValue, tree_id};

use super::{HEAD, LINKED, ROOT, SESSION, failed, parsed, ran, workspace};
use crate::changes::fingerprint;
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, NOW};
use crate::hooks::HookCall;
use crate::ports::FsOp;
use crate::project::Workspace;
use crate::trees::note_tree;

/// The session base, a commit before [`HEAD`].
const BASE: &str = "2222222222222222222222222222222222222222";

fn record_path(agent: &str) -> String {
    format!("{SESSION}/tree-{}-{agent}.json", tree_id(ROOT))
}

/// What git answers in a checkout at [`ROOT`]: its git dirs `dirs`, HEAD, the session base
/// `B` resolving to [`BASE`], the branch `branch` (`None`: detached), and whether [`BASE`] is
/// an ancestor of [`HEAD`].
struct Git<'a> {
    dirs: &'a str,
    branch: Option<&'a str>,
    ancestor: bool,
}

impl Git<'_> {
    fn process(&self) -> FakeProcess {
        let answer = |ok: bool| if ok { ran("") } else { failed() };
        FakeProcess::new()
            .answering(
                &["git", "rev-parse", "--git-dir", "--git-common-dir"],
                ROOT,
                ran(self.dirs),
            )
            .answering(
                &["git", "rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
                ROOT,
                ran(&format!("{HEAD}\n")),
            )
            .answering(
                &["git", "rev-parse", "--verify", "--quiet", "B^{commit}"],
                ROOT,
                ran(&format!("{BASE}\n")),
            )
            .answering(
                &["git", "rev-parse", "HEAD"],
                ROOT,
                ran(&format!("{HEAD}\n")),
            )
            .answering(
                &["git", "symbolic-ref", "--short", "-q", "HEAD"],
                ROOT,
                self.branch.map_or_else(failed, |b| ran(&format!("{b}\n"))),
            )
            .answering(
                &["git", "merge-base", "--is-ancestor", BASE, HEAD],
                ROOT,
                answer(self.ancestor),
            )
            .otherwise(failed())
    }
}

const MAIN_CHECKOUT: Git<'static> = Git {
    dirs: "/r/.git\n/r/.git\n",
    branch: Some("main"),
    ancestor: true,
};

const WORKTREE: Git<'static> = Git {
    dirs: LINKED,
    branch: Some("wt"),
    ancestor: true,
};

fn ports(files: FakeFiles, git: &Git<'_>) -> FakePorts {
    FakePorts {
        process: git.process(),
        ..FakePorts::new(files)
    }
}

/// `files` with the session's base recorded as `B`.
fn with_base(files: FakeFiles) -> FakeFiles {
    files.with_file(&format!("{SESSION}/base"), "B\n")
}

fn note(ports: &FakePorts, ws: &Workspace, input: &str) -> Result<(), FsOp> {
    let input = parsed(input);
    let env = RunEnv::at(ROOT, Posix);
    let call = HookCall {
        ports,
        env: &env,
        input: &input,
    };
    note_tree(&call, ws).map_err(|error| error.op())
}

fn record(ports: &FakePorts, agent: &str) -> JsValue {
    JsValue::from_json(&ports.files.text(&record_path(agent)).expect("the record")).expect("JSON")
}

fn fingerprint_from(ports: &FakePorts, base: &str) -> String {
    let env = RunEnv::at(ROOT, Posix);
    fingerprint(ports, &env, &workspace(true), Some(base)).to_string()
}

#[test]
fn the_record_names_the_root_head_branch_start_fingerprint_and_time() {
    let ports = ports(with_base(FakeFiles::new(Posix)), &MAIN_CHECKOUT);

    note(&ports, &workspace(true), r#"{"session_id":"s1"}"#).expect("written");

    let fp = fingerprint_from(&ports, HEAD);
    assert_eq!(
        ports.files.text(&record_path("main")),
        Some(format!(
            r#"{{"root":"{ROOT}","head":"{HEAD}","branch":"main","startFp":"{fp}","at":{}}}"#,
            NOW.unix_millis()
        )),
        "1.x's JSON.stringify of the record, with Date.now()"
    );
}

#[test]
fn each_agent_context_has_its_own_record_written_once() {
    let first = format!(r#"{{"root":"{ROOT}","at":1}}"#);
    let files = with_base(FakeFiles::new(Posix)).with_file(&record_path("main"), &first);
    let ports = ports(files, &MAIN_CHECKOUT);

    note(&ports, &workspace(true), r#"{"session_id":"s1"}"#).expect("kept");
    note(
        &ports,
        &workspace(true),
        r#"{"session_id":"s1","agent_id":"a 1"}"#,
    )
    .expect("written");

    assert_eq!(
        ports.files.text(&record_path("main")),
        Some(first),
        "an existing record is never rewritten"
    );
    assert_eq!(
        record(&ports, "agent-a_1").get("head"),
        Some(&JsValue::String(HEAD.to_owned())),
        "the agent's own record, its id made safe"
    );
}

#[test]
fn the_main_agent_entering_a_linked_worktree_widens_to_the_session_base() {
    let ports = ports(with_base(FakeFiles::new(Posix)), &WORKTREE);

    note(&ports, &workspace(true), r#"{"session_id":"s1"}"#).expect("written");

    let rec = record(&ports, "main");
    assert_eq!(rec.get("head"), Some(&JsValue::String(BASE.to_owned())));
    assert_eq!(
        rec.get("startFp"),
        Some(&JsValue::Null),
        "no starting point"
    );
    assert_eq!(rec.get("branch"), Some(&JsValue::String("wt".to_owned())));
}

#[test]
fn the_scope_is_not_widened_for_an_agent_a_session_record_or_a_base_not_held() {
    let session_record = format!("{SESSION}/tree-{}-session.json", tree_id(ROOT));
    let held_elsewhere = Git {
        ancestor: false,
        ..WORKTREE
    };
    for (files, git, input, agent, why) in [
        (
            FakeFiles::new(Posix),
            &WORKTREE,
            r#"{"session_id":"s1","agent_id":"a1"}"#,
            "agent-a1",
            "a subagent",
        ),
        (
            FakeFiles::new(Posix).with_file(&session_record, "{}"),
            &WORKTREE,
            r#"{"session_id":"s1"}"#,
            "main",
            "the session started here",
        ),
        (
            FakeFiles::new(Posix),
            &held_elsewhere,
            r#"{"session_id":"s1"}"#,
            "main",
            "the worktree does not hold the base",
        ),
        (
            FakeFiles::new(Posix),
            &MAIN_CHECKOUT,
            r#"{"session_id":"s1"}"#,
            "main",
            "the main checkout",
        ),
    ] {
        let ports = ports(with_base(files), git);
        note(&ports, &workspace(true), input).expect("written");
        let rec = record(&ports, agent);
        assert_eq!(
            rec.get("head"),
            Some(&JsValue::String(HEAD.to_owned())),
            "{why}"
        );
        let fp = fingerprint_from(&ports, HEAD);
        assert_eq!(rec.get("startFp"), Some(&JsValue::String(fp)), "{why}");
    }
}

#[test]
fn a_detached_or_empty_branch_is_null() {
    for branch in [None, Some("")] {
        let git = Git {
            branch,
            ..MAIN_CHECKOUT
        };
        let ports = ports(with_base(FakeFiles::new(Posix)), &git);
        note(&ports, &workspace(true), r#"{"session_id":"s1"}"#).expect("written");
        assert_eq!(
            record(&ports, "main").get("branch"),
            Some(&JsValue::Null),
            "{branch:?}"
        );
    }
}

#[test]
fn without_git_or_a_session_id_nothing_is_written() {
    for (git, input) in [
        (false, r#"{"session_id":"s1"}"#),
        (true, r#"{"session_id":""}"#),
        (true, "{}"),
    ] {
        let ports = ports(FakeFiles::new(Posix), &MAIN_CHECKOUT);
        note(&ports, &workspace(git), input).expect("nothing to write");
        assert_eq!(ports.files.paths(), Vec::<String>::new(), "{input}");
    }
}

#[test]
fn a_write_failure_is_the_handler_s_error() {
    let ports = ports(
        FakeFiles::new(Posix).with_read_only(SESSION),
        &MAIN_CHECKOUT,
    );

    let error = note(&ports, &workspace(true), r#"{"session_id":"s1"}"#);

    assert_eq!(error, Err(FsOp::Write), "1.x's writeAtomic throws (F2)");
}
