use temper_domain::JsValue;
use temper_domain::PathStyle::{Posix, Windows};

use super::{LivePids, live_pids};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess, Reply};
use crate::ports::{EntryKind, ExitKind, Stream};

/// The fakes' own pid ([`RunEnv::at`]).
const OWN: u32 = 4242;

fn ran(stdout: &str) -> Reply {
    Reply::Exit {
        status: ExitKind::Code(0),
        stdout: stdout.to_owned(),
    }
}

fn ps_answers(reply: Reply) -> FakeProcess {
    FakeProcess::new().answering(&["ps", "-A", "-o", "pid="], "/w", reply)
}

fn number(n: f64) -> JsValue {
    JsValue::Number(n)
}

/// `/proc` with this process and pid 77 in it, and entries that are not pids.
fn proc_files() -> FakeFiles {
    FakeFiles::new(Posix)
        .with_file("/proc/4242/status", "")
        .with_listing(
            "/proc",
            &[
                ("1", EntryKind::Dir),
                ("77", EntryKind::Dir),
                ("4242", EntryKind::Dir),
                ("self", EntryKind::Symlink),
                ("meminfo", EntryKind::File),
                ("0x10", EntryKind::Dir),
            ],
        )
}

#[test]
fn with_proc_the_pids_are_its_numeric_names_and_no_process_runs() {
    let ports = FakePorts {
        process: ps_answers(ran("1\n")),
        ..FakePorts::new(proc_files())
    };

    let pids = live_pids(&ports, &RunEnv::at("/w", Posix));

    let alive = [1.0, 77.0, 4242.0, 78.0, 16.0].map(|n| pids.alive(&number(n)));
    assert_eq!(alive, [true, true, true, false, false], "/proc's pid names");
    assert_eq!(ports.process.calls(), 0, "no ps when /proc answers");
}

#[test]
fn without_proc_one_ps_call_lists_the_pids() {
    let ports = FakePorts {
        process: ps_answers(ran("    1\n   77\n 4242\n")),
        ..FakePorts::new(FakeFiles::new(Posix))
    };

    let pids = live_pids(&ports, &RunEnv::at("/w", Posix));

    let alive = [1.0, 77.0, 4242.0, 78.0].map(|n| pids.alive(&number(n)));
    assert_eq!(alive, [true, true, true, false], "ps's pid column");
    assert_eq!(ports.process.calls(), 1, "one probe per call");
}

#[test]
fn a_proc_without_this_process_is_not_read() {
    // A `/proc` of another pid namespace: this process's directory is missing, so ps answers.
    let files = FakeFiles::new(Posix).with_listing("/proc", &[("1", EntryKind::Dir)]);
    let ports = FakePorts {
        process: ps_answers(ran("4242\n9\n")),
        ..FakePorts::new(files)
    };

    let pids = live_pids(&ports, &RunEnv::at("/w", Posix));

    assert_eq!(pids, LivePids::Known([9, OWN].into_iter().collect()));
}

#[test]
fn an_unlistable_proc_falls_back_to_ps() {
    let files = FakeFiles::new(Posix)
        .with_file("/proc/4242/status", "")
        .with_unlistable("/proc");
    let ports = FakePorts {
        process: ps_answers(ran("4242\n5\n")),
        ..FakePorts::new(files)
    };

    let pids = live_pids(&ports, &RunEnv::at("/w", Posix));

    assert_eq!(pids, LivePids::Known([5, OWN].into_iter().collect()));
}

#[test]
fn on_windows_one_tasklist_call_lists_the_pids_and_skips_other_lines() {
    let csv = concat!(
        "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n",
        "\"temper.exe\",\"4242\",\"Console\",\"1\",\"10,240 K\"\r\n",
        "\"a,b.exe\",\"616\",\"Console\",\"1\",\"1 K\"\r\n",
        "INFORMATION : aucune tâche ne correspond.\r\n",
        "\r\n",
    );
    let ports = FakePorts {
        process: FakeProcess::new().answering(
            &["tasklist", "/FO", "CSV", "/NH"],
            r"C:\w",
            ran(csv),
        ),
        ..FakePorts::new(FakeFiles::new(Windows))
    };

    let pids = live_pids(&ports, &RunEnv::at(r"C:\w", Windows));

    assert_eq!(pids, LivePids::Known([0, 616, OWN].into_iter().collect()));
    assert_eq!(
        ports.process.calls(),
        1,
        "one probe, and no /proc on Windows"
    );
}

#[test]
fn a_probe_that_cannot_answer_is_unknown() {
    let replies = [
        Reply::SpawnFails,
        Reply::Exit {
            status: ExitKind::Code(1),
            stdout: "4242\n".to_owned(),
        },
        Reply::Overflows(Stream::Stdout),
        // A busybox `ps` that ignores `-o pid=` and prints a table without this process.
        ran("PID   USER     TIME  COMMAND\n    1 root      0:00 init\n"),
        ran(""),
    ];

    let probes: Vec<LivePids> = replies
        .into_iter()
        .map(|reply| {
            let ports = FakePorts {
                process: ps_answers(reply),
                ..FakePorts::new(FakeFiles::new(Posix))
            };
            live_pids(&ports, &RunEnv::at("/w", Posix))
        })
        .collect();

    assert_eq!(
        probes,
        [
            LivePids::Unknown,
            LivePids::Unknown,
            LivePids::Unknown,
            LivePids::Unknown,
            LivePids::Unknown
        ]
    );
}

#[test]
fn unknown_reads_every_valid_pid_as_alive() {
    let alive = [1.0, 78.0, 2_147_483_647.0].map(|n| LivePids::Unknown.alive(&number(n)));

    assert_eq!(alive, [true, true, true], "nothing is deleted on a guess");
}

/// 1.x `pidAlive`: `Number.isInteger(pid) && pid > 0`, and `process.kill` throws (dead) on a
/// pid outside the int32 range.
#[test]
fn a_pid_that_is_not_a_positive_int32_is_dead_whatever_the_probe() {
    let not_pids = [
        number(0.0),
        number(-0.0),
        number(-1.0),
        number(1.5),
        number(2_147_483_648.0),
        number(f64::NAN),
        number(f64::INFINITY),
        JsValue::String("5".to_owned()),
        JsValue::Null,
        JsValue::Bool(true),
        JsValue::Array(vec![number(5.0)]),
    ];
    let known = LivePids::Known([0, 5, 2_147_483_647].into_iter().collect());

    let alive: Vec<(bool, bool)> = not_pids
        .iter()
        .map(|pid| (known.alive(pid), LivePids::Unknown.alive(pid)))
        .collect();

    assert_eq!(alive, vec![(false, false); not_pids.len()]);
    assert!(
        known.alive(&number(2_147_483_647.0)),
        "the int32 maximum is a pid"
    );
}
