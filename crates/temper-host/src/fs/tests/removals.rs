//! `remove_file`, `remove_all` and the modification time `stat` reports (port Phase 3d).

use std::fs;
use std::time::{Duration, SystemTime};

use temper_app::{Files, FsOp};

use super::{StdFiles, TestResult, symlink_dir};

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn stat_reports_the_modification_time_as_node_s_mtime_ms() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("f");
    fs::write(&file, "x")?;
    let when = SystemTime::UNIX_EPOCH + Duration::from_millis(1_700_000_000_250);
    fs::File::options().write(true).open(&file)?.set_modified(when)?;

    let meta = StdFiles.stat(&file)?.ok_or("the file is there")?;

    assert_eq!(
        meta.modified.map(|t| t.millis()),
        Some(1_700_000_000_250.0),
        "sec * 1e3 + nsec / 1e6"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn remove_file_removes_a_file_or_a_link_and_ignores_a_missing_one() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (file, target, link) = (
        dir.path().join("f"),
        dir.path().join("d"),
        dir.path().join("l"),
    );
    fs::write(&file, "x")?;
    fs::create_dir(&target)?;
    fs::write(target.join("kept"), "k")?;
    symlink_dir(&target, &link)?;

    StdFiles.remove_file(&file)?;
    StdFiles.remove_file(&file)?;
    let on_dir = StdFiles.remove_file(&target).map_err(|error| error.op());

    assert!(!file.exists(), "removed");
    assert_eq!(on_dir, Err(FsOp::Remove), "rmSync without recursive throws on a directory");
    assert!(target.join("kept").exists(), "the directory is untouched");
    #[cfg(unix)]
    {
        StdFiles.remove_file(&link)?;
        assert!(fs::symlink_metadata(&link).is_err(), "the link is gone");
        assert!(target.join("kept").exists(), "its target stays");
    }
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn remove_all_removes_a_tree_without_following_a_link() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (tree, outside) = (dir.path().join("tree"), dir.path().join("outside"));
    fs::create_dir_all(tree.join("a").join("b"))?;
    fs::write(tree.join("a").join("b").join("f"), "x")?;
    fs::create_dir(&outside)?;
    fs::write(outside.join("kept"), "k")?;
    symlink_dir(&outside, &tree.join("link"))?;
    let file = dir.path().join("file");
    fs::write(&file, "x")?;
    let link = dir.path().join("top-link");
    symlink_dir(&outside, &link)?;

    StdFiles.remove_all(&tree)?;
    StdFiles.remove_all(&tree)?;
    StdFiles.remove_all(&file)?;
    StdFiles.remove_all(&link)?;

    assert!(fs::symlink_metadata(&tree).is_err(), "the tree is gone");
    assert!(!file.exists(), "a file goes too");
    assert!(fs::symlink_metadata(&link).is_err(), "a link to a directory goes");
    assert!(outside.join("kept").exists(), "no link was followed");
    Ok(())
}
