use std::path::Path;

use temper_domain::PathStyle::Posix;
use temper_domain::{Config, SkillVia, TelemetryEvent};

use super::{ROTATE_AT, log};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts};
use crate::hooks::{HookCall, HookInput};
use crate::ports::Files;
use crate::project::Workspace;

const LOG: &str = "/s/telemetry.jsonl";
const ROTATED: &str = "/s/telemetry.1.jsonl";

fn workspace() -> Workspace {
    Workspace {
        root: "/ws".to_owned(),
        state_dir: "/s".to_owned(),
        config: Config::defaults(),
        config_problem: None,
        git: false,
    }
}

/// Logs one `skill-loaded` row for session `sid`.
fn log_for(ports: &FakePorts, sid: &str) -> bool {
    let input = HookInput::parse(&format!(r#"{{"session_id":"{sid}"}}"#)).expect("JSON");
    let event = TelemetryEvent::SkillLoaded {
        via: SkillVia::Skill,
    };
    let call = HookCall {
        ports,
        env: &RunEnv::at("/ws", Posix),
        input: &input,
    };
    log(&call, &workspace(), &event).is_ok()
}

/// The `sid` of each row of `text`, in order.
fn sids(text: Option<String>) -> Vec<String> {
    text.unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(r#""sid":""#)?;
            Some(rest.split_once('"')?.0.to_owned())
        })
        .collect()
}

fn len(text: Option<String>) -> u64 {
    text.map_or(0, |t| u64::try_from(t.len()).unwrap_or(u64::MAX))
}

/// Ported from `state-retention.test.mjs` "telemetry: below the cap it appends; at the cap it
/// rotates to telemetry.1.jsonl, keeping one old file", with rows told apart by session id
/// (the event names of the 1.x test arrive with their emitters).
#[test]
fn telemetry_below_the_cap_appends_and_at_the_cap_rotates_keeping_one_old_file() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let files = &ports.files;

    assert!(log_for(&ports, "s1") && log_for(&ports, "s1"), "two rows");
    assert_eq!(sids(files.text(LOG)), ["s1", "s1"], "appended in order");
    assert_eq!(files.text(ROTATED), None, "a small file is not rotated");

    let line = "{\"t\":\"x\",\"sid\":\"old\",\"event\":\"filler\"}\n";
    let lines = usize::try_from(ROTATE_AT)
        .expect("1 MiB fits")
        .div_ceil(line.len());
    let ports = FakePorts::new(FakeFiles::new(Posix).with_file(LOG, &line.repeat(lines)));
    let files = &ports.files;
    assert!(log_for(&ports, "s2"), "the row after the rotation");
    assert!(len(files.text(ROTATED)) >= ROTATE_AT, "the full file moved");
    assert_eq!(sids(files.text(LOG)), ["s2"], "a new file with one row");

    let head = "{\"sid\":\"marker\"}\n";
    let marker = format!("{head}{}", " ".repeat(1024 * 1024 + 10 - head.len()));
    files
        .write_atomic(Path::new(LOG), &marker)
        .expect("the fake writes");
    assert!(log_for(&ports, "s3"), "the row after the second rotation");
    assert_eq!(
        [sids(files.text(ROTATED)), sids(files.text(LOG))],
        [vec!["marker".to_owned()], vec!["s3".to_owned()]],
        "the oldest file is dropped on the second rotation"
    );
}

#[test]
fn a_state_dir_that_cannot_be_made_loses_the_row() {
    let ports = FakePorts::new(FakeFiles::new(Posix).with_read_only("/s"));

    assert!(
        !log_for(&ports, "s1"),
        "the error reaches the caller, which drops it"
    );
    assert_eq!(ports.files.paths(), Vec::<String>::new(), "nothing written");
}
