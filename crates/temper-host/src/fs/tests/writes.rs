//! The `Files` writes and `stat`, against a temporary directory.

use std::fs;

use temper_app::{FileMeta, Files, FsOp};

use super::TestResult;
use crate::StdFiles;

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn write_atomic_makes_the_parents_and_replaces_the_file_leaving_nothing_beside_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("a").join("b").join("marker");

    StdFiles.write_atomic(&file, "one")?;
    StdFiles.write_atomic(&file, "two")?;

    assert_eq!(fs::read_to_string(&file)?, "two", "the last text");
    let names: Vec<_> = fs::read_dir(dir.path().join("a").join("b"))?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    assert_eq!(names, ["marker"], "no temporary file left");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn write_atomic_under_a_file_fails() -> TestResult {
    let dir = tempfile::tempdir()?;
    let blocker = dir.path().join("data");
    fs::write(&blocker, "")?;

    let failed = StdFiles.write_atomic(&blocker.join("sessions").join("m"), "x");

    assert_eq!(
        failed.map_err(|e| e.op()).err(),
        Some(FsOp::CreateDir),
        "a file stands where a parent directory must go"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn append_line_creates_the_file_and_adds_one_line_per_call() -> TestResult {
    let dir = tempfile::tempdir()?;
    let log = dir.path().join("telemetry.jsonl");

    StdFiles.append_line(&log, "{\"a\":1}")?;
    StdFiles.append_line(&log, "{\"b\":2}")?;

    assert_eq!(
        fs::read_to_string(&log)?,
        "{\"a\":1}\n{\"b\":2}\n",
        "appended"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn create_dir_all_rename_and_stat() -> TestResult {
    let dir = tempfile::tempdir()?;
    let state = dir.path().join("x").join("temper");
    let (log, old) = (
        state.join("telemetry.jsonl"),
        state.join("telemetry.1.jsonl"),
    );

    StdFiles.create_dir_all(&state)?;
    StdFiles.create_dir_all(&state)?;
    fs::write(&log, "12345")?;
    fs::write(&old, "old")?;
    let before = StdFiles.stat(&log)?;
    StdFiles.rename(&log, &old)?;

    assert_eq!(before, Some(FileMeta { len: 5 }), "the size");
    assert_eq!(StdFiles.stat(&log)?, None, "moved away");
    assert_eq!(fs::read_to_string(&old)?, "12345", "replaced");
    assert_eq!(
        StdFiles.rename(&log, &old).map_err(|e| e.op()).err(),
        Some(FsOp::Rename),
        "nothing to move"
    );
    Ok(())
}
