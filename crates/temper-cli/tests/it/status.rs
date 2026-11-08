//! `temper status` (text) end to end, with real git (port Phase 3b).

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::support::{TestResult, git, temper_command};

const MANIFEST: &str = "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

/// `temper status <args>` in `dir`, without the caller's `TEMPER_BASE` unless `base` sets it:
/// (exit code, stdout, stderr).
fn status_in(
    dir: &Path,
    args: &[&str],
    base: Option<&str>,
) -> Result<(Option<i32>, String, String), Box<dyn std::error::Error>> {
    let mut command = temper_command();
    command.arg("status").args(args).current_dir(dir);
    if let Some(base) = base {
        command.env("TEMPER_BASE", base);
    }
    let output = command.output()?;
    Ok((
        output.status.code(),
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

/// A committed one-crate repository whose commit has the same id every time (fixed dates), so
/// two such repositories with the same changes have the same fingerprint.
fn repo() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    fs::write(dir.path().join("Cargo.toml"), MANIFEST)?;
    fs::create_dir_all(dir.path().join("src"))?;
    fs::write(dir.path().join("src/lib.rs"), "pub fn f() {}\n")?;
    git(dir.path(), &["init", "-q"])?;
    git(dir.path(), &["add", "."])?;
    let committed = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(["-c", "commit.gpgsign=false", "commit", "-qm", "init"])
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .current_dir(dir.path())
        .status()?;
    if !committed.success() {
        return Err("git commit failed".into());
    }
    Ok(dir)
}

fn fingerprint(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("fingerprint: "))
}

#[test]
fn outside_a_rust_project_status_exits_2() -> TestResult {
    let dir = tempfile::tempdir()?;

    let run = status_in(dir.path(), &[], None)?;

    assert_eq!(
        run,
        (
            Some(2),
            String::new(),
            "temper: not inside a Rust project (no Cargo.toml found)\n".to_owned()
        )
    );
    Ok(())
}

/// `--session` without an id, or followed by a flag, prints the usage line and exits 2,
/// before the project is looked for.
#[test]
fn a_session_flag_without_an_id_is_a_usage_error() -> TestResult {
    let dir = tempfile::tempdir()?;
    for args in [&["--session"][..], &["--session", "--x"]] {
        let run = status_in(dir.path(), args, None)?;
        assert_eq!(
            run,
            (
                Some(2),
                String::new(),
                "usage: temper status [--json] [--session ID]\n".to_owned()
            ),
            "{args:?}"
        );
    }
    Ok(())
}

/// Ported from `status-json.test.mjs`: "status: the text output keeps its lines".
#[test]
fn the_text_output_keeps_its_lines() -> TestResult {
    let dir = repo()?;

    let (code, stdout, stderr) = status_in(dir.path(), &[], None)?;

    assert_eq!(code, Some(0), "{stderr}");
    let keys: Vec<&str> = stdout
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.split(':').next().unwrap_or(line))
        .collect();
    assert_eq!(
        keys,
        [
            "root",
            "state",
            "base",
            "fingerprint",
            "changed",
            "gate",
            "review",
            "baseline",
            "config"
        ]
    );
    Ok(())
}

/// Item 18 end to end: a `TEMPER_BASE` that is no commit here gives HEAD and the note.
#[test]
fn a_garbage_temper_base_gives_head_and_a_note() -> TestResult {
    let dir = repo()?;

    let (code, stdout, stderr) = status_in(dir.path(), &[], Some("not-a-commit-at-all"))?;

    assert_eq!(code, Some(0));
    assert_eq!(
        stderr,
        "temper: TEMPER_BASE not-a-commit is not a commit in this repository (another session's base?); using HEAD.\n"
    );
    assert!(!stdout.contains("not-a-commit"), "{stdout}");
    Ok(())
}

/// With stdout and stderr on one pipe (a terminal, `2>&1`), the `TEMPER_BASE` note comes
/// before the nine lines, as 1.x writes it while it finds the base (#207 review F1).
#[test]
fn the_temper_base_note_comes_before_the_lines_on_a_merged_stream() -> TestResult {
    let dir = repo()?;
    let (mut reader, writer) = std::io::pipe()?;
    let mut child = temper_command()
        .arg("status")
        .current_dir(dir.path())
        .env("TEMPER_BASE", "not-a-commit-at-all")
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .spawn()?;
    let mut merged = String::new();
    std::io::Read::read_to_string(&mut reader, &mut merged)?;
    let status = child.wait()?;

    assert!(status.success(), "{merged}");
    assert!(
        merged.starts_with("temper: TEMPER_BASE not-a-commit is not a commit"),
        "{merged}"
    );
    assert!(merged.contains("\nroot:        "), "{merged}");
    Ok(())
}

/// The fingerprint of `dir` with `tests/<name>` untracked, made by `make`.
fn fingerprint_with(
    name: &str,
    make: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<String, Box<dyn std::error::Error>> {
    let dir = repo()?;
    fs::create_dir_all(dir.path().join("tests"))?;
    make(&dir.path().join("tests").join(name))?;
    let (code, stdout, stderr) = status_in(dir.path(), &[], None)?;
    assert_eq!(code, Some(0), "{stderr}");
    Ok(fingerprint(&stdout).ok_or("a fingerprint line")?.to_owned())
}

#[cfg(unix)]
fn mkfifo(path: &Path) -> std::io::Result<()> {
    let made = Command::new("mkfifo").arg(path).status()?;
    if made.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("mkfifo failed"))
    }
}

/// OC-1 A (b), wi-50fc9a, D16(t): an untracked link to `/dev/zero` (1.x reads it to 2 GiB) and
/// a link to a directory hash their frame only: the fingerprint of an empty file of the same
/// name, whose content adds nothing. git does not list a FIFO, so it adds nothing at all.
#[cfg(unix)]
#[test]
fn untracked_entries_that_are_not_files_hash_their_frame_only() -> TestResult {
    let empty = fingerprint_with("x.rs", |path| fs::write(path, ""))?;
    let nothing = fingerprint_with("x.rs", |_| Ok(()))?;
    let zero = fingerprint_with("x.rs", |path| std::os::unix::fs::symlink("/dev/zero", path))?;
    let to_dir = fingerprint_with("x.rs", |path| {
        let parent = path.parent().ok_or(std::io::ErrorKind::NotFound)?;
        std::os::unix::fs::symlink(parent, path)
    })?;
    let fifo = fingerprint_with("x.rs", mkfifo)?;

    assert_eq!([&zero, &to_dir], [&empty, &empty]);
    assert_eq!(fifo, nothing, "git does not list a FIFO");
    Ok(())
}

/// Outside git the walk meets a FIFO, which 1.x blocks on until the hook's timeout; Rust
/// hashes its frame only, as for an empty file (OC-1 A (b), D16(t)).
#[cfg(unix)]
#[test]
fn outside_git_a_fifo_hashes_its_frame_only_without_blocking() -> TestResult {
    let walked = |make: &dyn Fn(&Path) -> std::io::Result<()>| -> Result<String, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("Cargo.toml"), MANIFEST)?;
        make(&dir.path().join("x.rs"))?;
        let (code, stdout, stderr) = status_in(dir.path(), &[], None)?;
        assert_eq!(code, Some(0), "{stderr}");
        Ok(fingerprint(&stdout).ok_or("a fingerprint line")?.to_owned())
    };

    let empty = walked(&|path| fs::write(path, ""))?;
    let fifo = walked(&mkfifo)?;

    assert_eq!(fifo, empty);
    Ok(())
}

/// A file over Node's 2^31 - 1 byte limit hashes its frame only, as 1.x's `readFileSync`
/// throws there (a sparse file costs no disk).
#[test]
fn an_untracked_file_over_two_gib_hashes_its_frame_only() -> TestResult {
    let empty = fingerprint_with("big.rs", |path| fs::write(path, ""))?;
    let sparse = fingerprint_with("big.rs", |path| fs::File::create(path)?.set_len(1 << 31))?;

    assert_eq!(sparse, empty);
    Ok(())
}

/// An untracked name that is not UTF-8: git's `-z` list is decoded lossily, so 1.x reads the
/// file through a name with U+FFFD, finds nothing, and hashes `\n?tests/\u{FFFD}.rs\n` alone:
/// the fingerprint of an empty file with that name. Linux only: macOS and Windows refuse such
/// names.
#[cfg(target_os = "linux")]
#[test]
fn an_untracked_name_that_is_not_utf8_hashes_its_lossy_frame() -> TestResult {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let lossy = fingerprint_with("\u{fffd}.rs", |path| fs::write(path, ""))?;
    let raw = fingerprint_with("x", |path| {
        let dir = path.parent().ok_or(std::io::ErrorKind::NotFound)?;
        fs::write(
            dir.join(OsStr::from_bytes(b"\xff.rs")),
            "content 1.x cannot reach",
        )
    })?;

    assert_eq!(raw, lossy);
    Ok(())
}
