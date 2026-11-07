//! `read_into`: a regular file streamed whole, and everything else answered without reading
//! (OC-1 A (b), wi-50fc9a).

use std::fs;

use temper_app::{Files, ReadInto};

use super::super::StdFiles;
use super::TestResult;

/// 1.x's `readFileSync` limit: `kIoMaxLength`, 2^31 - 1 bytes.
const NODE_LIMIT: u64 = (1 << 31) - 1;

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn a_regular_file_is_streamed_whole_as_bytes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("a.rs");
    fs::write(&file, [0x66, 0xff, 0x0a])?;
    let mut sink = Vec::new();

    let read = StdFiles.read_into(&file, NODE_LIMIT, &mut sink)?;

    assert_eq!((read, sink), (ReadInto::Read, vec![0x66, 0xff, 0x0a]));
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn a_missing_file_and_a_directory_are_not_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut sink = Vec::new();

    let answers = [
        StdFiles.read_into(&dir.path().join("missing"), NODE_LIMIT, &mut sink)?,
        StdFiles.read_into(dir.path(), NODE_LIMIT, &mut sink)?,
    ];

    assert_eq!(answers, [ReadInto::NotFound, ReadInto::NotAFile]);
    assert!(sink.is_empty(), "nothing written");
    Ok(())
}

/// 1.x throws `ERR_FS_FILE_TOO_LARGE` past 2^31 - 1 bytes; a sparse file of 2^31 bytes costs
/// no disk and is refused before a byte is read.
#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn a_file_over_the_limit_is_too_large() -> TestResult {
    let dir = tempfile::tempdir()?;
    let sparse = dir.path().join("sparse.rs");
    fs::File::create(&sparse)?.set_len(1 << 31)?;
    let small = dir.path().join("small.rs");
    fs::write(&small, b"12345")?;
    let mut sink = Vec::new();

    let answers = [
        StdFiles.read_into(&sparse, NODE_LIMIT, &mut sink)?,
        StdFiles.read_into(&small, 4, &mut sink)?,
        StdFiles.read_into(&small, 5, &mut Vec::new())?,
    ];

    assert_eq!(
        answers,
        [ReadInto::TooLarge, ReadInto::TooLarge, ReadInto::Read]
    );
    Ok(())
}

/// A FIFO would block `open` until a writer came (1.x hangs there until the hook times out);
/// a link to `/dev/zero` would stream forever. Both are refused from their metadata.
#[cfg(unix)]
#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn a_fifo_and_a_link_to_a_device_are_not_a_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fifo = dir.path().join("fifo.rs");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status()?;
    assert!(made.success(), "mkfifo");
    let zero = dir.path().join("zero.rs");
    std::os::unix::fs::symlink("/dev/zero", &zero)?;
    let mut sink = Vec::new();

    let answers = [
        StdFiles.read_into(&fifo, NODE_LIMIT, &mut sink)?,
        StdFiles.read_into(&zero, NODE_LIMIT, &mut sink)?,
    ];

    assert_eq!(answers, [ReadInto::NotAFile, ReadInto::NotAFile]);
    assert!(sink.is_empty(), "nothing read");
    Ok(())
}
