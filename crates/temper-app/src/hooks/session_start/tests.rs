use std::path::PathBuf;

use temper_domain::PathStyle::{self, Posix, Windows};

use super::{gate_command, plugin_cli, write_env_file};
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts};

const ENV: &str = "/w/.temper/env.sh";

fn env(plugin_root: Option<&str>, bin: Option<&str>) -> RunEnv {
    RunEnv {
        plugin_root: plugin_root.map(PathBuf::from),
        env_file: Some(PathBuf::from(ENV)),
        temper_bin: bin.map(PathBuf::from),
        ..RunEnv::at("/w", Posix)
    }
}

#[test]
fn the_env_file_gains_temper_cli_and_temper_base_then_d17_s_lines() {
    let ports = FakePorts::new(FakeFiles::new(Posix).with_file(ENV, "export A=1\n"));
    let env = env(Some("/p/temper"), Some("/opt/t/temper"));

    assert!(write_env_file(&ports, &env, Some("1111")), "written");

    assert_eq!(
        ports.files.text(ENV).as_deref(),
        Some(
            "export A=1\nexport TEMPER_CLI=\"/p/temper/scripts/temper.mjs\"\nexport TEMPER_BASE=\"1111\"\n\
             export TEMPER_BIN=\"/opt/t/temper\"\nexport PATH=\"/opt/t:$PATH\"\n"
        ),
        "appended in one write"
    );
}

#[test]
fn without_an_env_file_or_anything_to_export_nothing_is_written() {
    let ports = FakePorts::new(FakeFiles::new(Posix));
    let unset = RunEnv {
        env_file: None,
        ..env(Some("/p"), None)
    };

    assert!(!write_env_file(&ports, &unset, Some("1111")), "no CLAUDE_ENV_FILE");
    assert!(!write_env_file(&ports, &env(None, None), None), "nothing to export");
    assert!(ports.files.paths().is_empty(), "no file");
}

#[test]
fn an_unwritable_env_file_is_ignored() {
    let ports = FakePorts::new(FakeFiles::new(Posix).with_read_only("/w/.temper"));

    assert!(!write_env_file(&ports, &env(Some("/p"), None), None), "1.x's catch");
}

#[test]
fn the_cli_path_is_path_join_of_the_plugin_root() {
    let cli = |style: PathStyle, root: &str| plugin_cli(style, root);
    assert_eq!(
        [
            cli(Posix, "/p"),
            cli(Posix, "/p/"),
            cli(Posix, "/p/x/.."),
            cli(Posix, ""),
            cli(Posix, "rel"),
            cli(Windows, r"C:\p"),
            cli(Windows, ""),
        ],
        [
            "/p/scripts/temper.mjs",
            "/p/scripts/temper.mjs",
            "/p/scripts/temper.mjs",
            "scripts/temper.mjs",
            "rel/scripts/temper.mjs",
            r"C:\p\scripts\temper.mjs",
            r"scripts\temper.mjs",
        ],
        "Node's path.join(root, 'scripts', 'temper.mjs')"
    );
}

#[test]
fn the_gate_command_names_the_cli_or_the_binary_without_a_plugin_root() {
    assert_eq!(
        [
            gate_command(&env(Some("/p"), None)),
            gate_command(&env(None, Some("/t"))),
        ],
        ["node \"/p/scripts/temper.mjs\" gate", "temper gate"],
        "1.x's gateCmd, and OC-6 A's form"
    );
}
