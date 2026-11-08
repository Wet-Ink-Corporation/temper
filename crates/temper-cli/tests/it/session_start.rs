//! `temper hook session-start` end to end, with real git (port Phase 3d). Ported from 1.x
//! `handlers.test.mjs`'s `SessionStart` tests and `state-retention.test.mjs`'s "`SessionStart`
//! startup prunes old records but keeps the tree it starts on; resume prunes nothing".

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use temper_domain::JsValue;

use crate::support::{TestResult, git, hook_in, write_tree};

const MANIFEST: &str = "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

/// A committed one-crate repository.
fn repo() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_tree(
        dir.path(),
        &[("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn f() {}\n")],
    )?;
    git(dir.path(), &["init", "-q"])?;
    git(dir.path(), &["config", "core.autocrlf", "false"])?;
    git(dir.path(), &["add", "-A"])?;
    git(dir.path(), &["commit", "-qm", "base"])?;
    Ok(dir)
}

fn head(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}

fn state(dir: &Path) -> PathBuf {
    dir.join(".git").join("temper")
}

fn session(dir: &Path, sid: &str) -> PathBuf {
    state(dir).join("sessions").join(sid)
}

/// `hook session-start` for session `sid` from `source`, with `env`: (stdout as JSON, or
/// `Null` when empty; stderr).
fn start(
    dir: &Path,
    sid: &str,
    source: &str,
    env: &[(&str, &OsStr)],
) -> Result<(JsValue, String), Box<dyn std::error::Error>> {
    let input = format!(
        r#"{{"cwd":{},"session_id":"{sid}","source":"{source}"}}"#,
        JsValue::String(dir.to_string_lossy().into_owned()).to_json()
    );
    let mut vars: Vec<(&str, &OsStr)> = vec![("CLAUDE_PLUGIN_DATA", dir.as_os_str())];
    vars.extend_from_slice(env);
    let run = hook_in(dir, "session-start", &input, &vars)?;
    if run.code != Some(0) {
        return Err(format!("exit {:?}: {}", run.code, run.stderr).into());
    }
    let json = if run.stdout.is_empty() {
        JsValue::Null
    } else {
        JsValue::from_json(&run.stdout)?
    };
    Ok((json, run.stderr))
}

fn context(json: &JsValue) -> String {
    json.get("hookSpecificOutput")
        .and_then(|h| h.get("additionalContext"))
        .and_then(JsValue::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The fingerprint `temper status --session <sid>` prints.
fn session_fingerprint(dir: &Path, sid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new(env!("CARGO_BIN_EXE_temper"))
        .args(["status", "--session", sid])
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", std::env::temp_dir())
        .output()?;
    let stdout = String::from_utf8(out.stdout)?;
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix("fingerprint: "))
        .ok_or("a fingerprint line")?;
    Ok(line.to_owned())
}

#[test]
fn records_the_session_base_and_start_fingerprint_and_describes_the_state() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let head = head(dir)?;

    let (json, stderr) = start(dir, "s-start", "startup", &[])?;

    let text = context(&json);
    assert_eq!(
        json.get("hookSpecificOutput")
            .and_then(|h| h.get("hookEventName")),
        Some(&JsValue::String("SessionStart".to_owned()))
    );
    assert!(text.contains("governed by the temper plugin and the rust-craft engineering standard"));
    assert!(text.contains("It has not been loaded in this session yet."));
    assert!(text.contains("- Iron rules (production code): H-01 no unsafe; "));
    assert!(
        text.contains(&format!(
            "- Session base revision: {head}. Current state: no Rust changes since the session base."
        )),
        "{text}"
    );
    assert_eq!(
        json.get("systemMessage"),
        None,
        "on a current runtime, no warning"
    );
    assert_eq!(stderr, "");
    let sd = session(dir, "s-start");
    assert_eq!(fs::read_to_string(sd.join("base"))?, head);
    assert!(sd.join("started").exists());
    assert_eq!(
        fs::read_to_string(sd.join("start-fp"))?,
        session_fingerprint(dir, "s-start")?,
        "fingerprint(proj, head)"
    );
    Ok(())
}

#[test]
fn resume_keeps_the_base_and_startup_clear_and_fork_reset_it() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let first = head(dir)?;
    start(dir, "s-src", "startup", &[])?;
    fs::write(dir.join("src/lib.rs"), "pub fn g() {}\n")?;
    git(dir, &["commit", "-qam", "two"])?;
    let second = head(dir)?;
    let base = || fs::read_to_string(session(dir, "s-src").join("base"));

    for source in ["resume", "compact"] {
        let (json, _) = start(dir, "s-src", source, &[])?;
        let text = context(&json);
        assert_eq!(base()?, first, "{source}");
        let said = format!(
            "Session base revision: {first}. Current state: 1 Rust file(s) changed; gate not run; review none (fingerprint "
        );
        assert!(text.contains(&said), "{source}: {text}");
    }
    for source in ["clear", "fork", "startup"] {
        start(dir, "s-src", source, &[])?;
        assert_eq!(base()?, second, "{source}");
    }
    Ok(())
}

#[test]
fn reports_the_cached_gate_and_the_recorded_review_for_the_tree() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    fs::write(dir.join("src/lib.rs"), "pub fn g() {}\n")?;
    start(dir, "s-state", "startup", &[])?;
    let fp = session_fingerprint(dir, "s-state")?;
    let record = |ok: bool| format!(r#"{{"ok":{ok},"fp":"{fp}","steps":[]}}"#);
    let gate = format!("gate/{fp}.json");
    let verdict = format!("verdicts/{fp}.json");
    let (green_record, red_record) = (record(true), record(false));
    write_tree(
        &state(dir),
        &[
            (gate.as_str(), green_record.as_str()),
            (verdict.as_str(), r#"{"verdict":"changes-requested"}"#),
        ],
    )?;

    let (green, _) = start(dir, "s-state", "resume", &[])?;
    write_tree(&state(dir), &[(gate.as_str(), red_record.as_str())])?;
    let (red, _) = start(dir, "s-state", "resume", &[])?;

    assert!(
        context(&green).contains(&format!(
            "Current state: 1 Rust file(s) changed; gate green; review changes-requested (fingerprint {fp})."
        )),
        "{}",
        context(&green)
    );
    assert!(context(&red).contains("; gate red; "));
    Ok(())
}

#[test]
fn outside_a_rust_project_it_does_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;

    let (json, stderr) = start(dir.path(), "s-none", "startup", &[])?;

    assert_eq!(
        (json, stderr),
        (JsValue::Null, String::new()),
        "1.x's `{{}}`"
    );
    assert_eq!(fs::read_dir(dir.path())?.count(), 0, "nothing written");
    Ok(())
}

/// Ported from `handlers.test.mjs` "`SessionStart`: `CLAUDE_ENV_FILE` gains `TEMPER_CLI` and
/// `TEMPER_BASE`", its two line assertions unchanged; D17's lines follow (OC-6 A).
#[test]
fn the_env_file_gains_temper_cli_and_temper_base_then_d17_s_lines() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let env_file = dir.join(".temper").join("env.sh");
    write_tree(dir, &[(".temper/env.sh", "")])?;
    let plugin = dir.join("plugin");

    start(
        dir,
        "s-env",
        "startup",
        &[
            ("CLAUDE_ENV_FILE", env_file.as_os_str()),
            ("CLAUDE_PLUGIN_ROOT", plugin.as_os_str()),
        ],
    )?;

    let text = fs::read_to_string(&env_file)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let first = lines.first().copied().unwrap_or_default();
    assert!(
        first.starts_with("export TEMPER_CLI=\"") && first.ends_with("/scripts/temper.mjs\""),
        "{first}"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(format!("export TEMPER_BASE=\"{}\"", head(dir)?).as_str())
    );
    let (bin, path) = d17_lines()?;
    let written: Option<Vec<String>> = lines
        .get(2..4)
        .map(|got| got.iter().map(|line| drive_folded(line)).collect());
    assert_eq!(written, Some(vec![drive_folded(&bin), path]), "{text}");
    assert_eq!(lines.get(4).copied(), Some(""), "one newline after each");
    Ok(())
}

/// `line` with a Windows drive letter after `export TEMPER_BIN="` in lower case:
/// `current_exe` and `canonicalize` may spell the drive in different cases there. The `PATH`
/// line's `/c/x` form is lower case already.
fn drive_folded(line: &str) -> String {
    const HEAD: &str = "export TEMPER_BIN=\"";
    match line.strip_prefix(HEAD).map(str::as_bytes) {
        Some([drive, b':', ..]) if cfg!(windows) && drive.is_ascii_alphabetic() => {
            let mut out = line.to_owned();
            out.get_mut(HEAD.len()..=HEAD.len())
                .map(str::make_ascii_lowercase);
            out
        }
        _ => line.to_owned(),
    }
}

/// D17's two lines (OC-6 A) for this test's binary: `TEMPER_BIN` is its real path and `PATH`
/// gains its directory, each with `/` separators and quoted for the shell, the directory in
/// Git Bash's `/c/x` form on Windows.
fn d17_lines() -> Result<(String, String), Box<dyn std::error::Error>> {
    let exe = fs::canonicalize(env!("CARGO_BIN_EXE_temper"))?;
    let dir = exe.parent().ok_or("the binary has a directory")?;
    let bin = slashed(&exe);
    let dir = slashed(dir);
    let dir = match dir.as_bytes() {
        [drive, b':', rest @ ..] if cfg!(windows) && drive.is_ascii_alphabetic() => {
            let rest = std::str::from_utf8(rest)?;
            let rest = if rest.is_empty() { "/" } else { rest };
            format!("/{}{rest}", char::from(drive.to_ascii_lowercase()))
        }
        _ => dir,
    };
    Ok((
        format!("export TEMPER_BIN=\"{}\"", shell_quoted(&bin)),
        format!("export PATH=\"{}:$PATH\"", shell_quoted(&dir)),
    ))
}

/// `path` with `/` separators, without the `\\?\` prefix `canonicalize` gives on Windows,
/// which `current_exe` does not.
fn slashed(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.replace('\\', "/")
}

/// `"`, `$` and `` ` `` escaped with a backslash, as inside a double-quoted shell word.
fn shell_quoted(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        if matches!(c, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
        out
    })
}

#[test]
fn a_runtime_warning_goes_to_the_human_and_the_model() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let warning = "temper: Node 17.9.1 is older than the documented minimum, Node 18.0.0.";
    let warnings = format!("{warning}\n\n");

    let (json, _) = start(
        dir,
        "s-ver",
        "startup",
        &[("TEMPER_RUNTIME_WARNINGS", OsStr::new(&warnings))],
    )?;

    assert_eq!(
        json.get("systemMessage").and_then(JsValue::as_str),
        Some(warning)
    );
    assert!(context(&json).ends_with(&format!("\n- {warning}")));
    Ok(())
}

fn age(path: &Path, days: u64) -> TestResult {
    let when = SystemTime::now() - Duration::from_secs(days * 86_400);
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(when)?;
    Ok(())
}

fn names(dir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut names = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[test]
fn startup_prunes_old_records_but_keeps_the_tree_it_starts_on_and_resume_prunes_nothing()
-> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    fs::write(dir.join("src/lib.rs"), "pub fn uncommitted() {}\n")?;
    // The tree's fingerprint against HEAD, which the startup session records as its start.
    start(dir, "s-probe", "resume", &[])?;
    let cur = session_fingerprint(dir, "s-probe")?;
    let old = "0000000000000001";
    let records = [
        format!("verdicts/{cur}.json"),
        format!("gate/{cur}.json"),
        format!("verdicts/{old}.json"),
        format!("gate/{old}.json"),
        format!("logs/{old}-test.log"),
    ];
    for rel in &records {
        write_tree(&state(dir), &[(rel.as_str(), "{}")])?;
        age(&state(dir).join(rel), 400)?;
    }

    start(dir, "s-resume", "resume", &[])?;
    let kept = state(dir).join(format!("verdicts/{old}.json")).exists();
    start(dir, "s-start", "startup", &[])?;

    assert!(kept, "resume does not prune");
    assert_eq!(
        names(&state(dir).join("verdicts"))?,
        [format!("{cur}.json")]
    );
    assert_eq!(names(&state(dir).join("gate"))?, [format!("{cur}.json")]);
    assert_eq!(names(&state(dir).join("logs"))?, Vec::<String>::new());
    assert!(session(dir, "s-start").exists());
    Ok(())
}

#[test]
fn the_telemetry_row_carries_the_source_as_given() -> TestResult {
    let repo = repo()?;
    let dir = repo.path();
    let cwd = JsValue::String(dir.to_string_lossy().into_owned()).to_json();
    let data: [(&str, &OsStr); 1] = [("CLAUDE_PLUGIN_DATA", dir.as_os_str())];
    for input in [
        format!(r#"{{"cwd":{cwd},"session_id":"s1"}}"#),
        format!(r#"{{"cwd":{cwd},"session_id":"s1","source":5}}"#),
        format!(r#"{{"cwd":{cwd},"session_id":"s1","source":"resume"}}"#),
    ] {
        hook_in(dir, "session-start", &input, &data)?;
    }

    let log = fs::read_to_string(state(dir).join("telemetry.jsonl"))?;
    let tails: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_once(r#""event":"session-start""#))
        .map(|(_, tail)| tail)
        .collect();
    assert_eq!(tails, ["}", r#","source":5}"#, r#","source":"resume"}"#]);
    Ok(())
}
