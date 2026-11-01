use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;

use temper_app::{EntryKind, Files};

use super::{StdFiles, read_capped_text};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn read_text_decodes_lossily_and_keeps_a_bom() -> TestResult {
    let dir = tempfile::tempdir()?;
    let broken = dir.path().join("broken.rs");
    let bom = dir.path().join("bom.rs");
    fs::write(
        &broken,
        [0x66, 0x80, 0xE2, 0x82, 0x41, 0xF0, 0x9F, 0x98, 0xC3],
    )?;
    fs::write(&bom, [0xEF, 0xBB, 0xBF, 0x78])?;

    let texts = [StdFiles.read_text(&broken)?, StdFiles.read_text(&bom)?];

    assert_eq!(
        texts,
        [
            Some("f\u{FFFD}\u{FFFD}A\u{FFFD}\u{FFFD}".to_owned()),
            Some("\u{feff}x".to_owned())
        ],
        "as Node's readFileSync(path, 'utf8') decodes them"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn read_text_of_a_missing_file_is_none() -> TestResult {
    let dir = tempfile::tempdir()?;

    let text = StdFiles.read_text(&dir.path().join("missing.rs"))?;

    assert_eq!(text, None, "no such file");
    Ok(())
}

#[test]
fn text_longer_than_a_js_string_is_too_large() {
    let read = |bytes: &str, max_units| {
        read_capped_text(bytes.as_bytes(), max_units).map_err(|error| error.kind())
    };

    assert_eq!(
        [
            read("abcdefg", 2),
            read("abc", 2),
            read("\u{1F600}", 1),
            read("ab", 2),
            read("\u{e9}\u{20ac}", 2),
        ],
        [
            Err(io::ErrorKind::FileTooLarge),
            Err(io::ErrorKind::FileTooLarge),
            Err(io::ErrorKind::FileTooLarge),
            Ok("ab".to_owned()),
            Ok("\u{e9}\u{20ac}".to_owned()),
        ],
        "past 3 bytes per unit, or past the UTF-16 units, the text is too large"
    );
}

#[cfg(unix)]
#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn read_dir_lists_names_in_byte_order() -> TestResult {
    let dir = tempfile::tempdir()?;
    for name in [
        "b.rs",
        "\u{e9}.rs",
        "Z.rs",
        "9.rs",
        "a.rs",
        "_x.rs",
        "10.rs",
        // Not "A.rs": macOS file systems are case-insensitive, so it would replace "a.rs".
        "C.rs",
    ] {
        fs::write(dir.path().join(name), "")?;
    }
    fs::create_dir(dir.path().join("sub"))?;

    let names: Vec<OsString> = StdFiles
        .read_dir(dir.path())?
        .into_iter()
        .map(|entry| entry.name)
        .collect();

    assert_eq!(
        names,
        [
            "10.rs",
            "9.rs",
            "C.rs",
            "Z.rs",
            "_x.rs",
            "a.rs",
            "b.rs",
            "sub",
            "\u{e9}.rs"
        ],
        "libuv's scandir order, as Node 22 lists them"
    );
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn read_dir_reports_entry_kinds() -> TestResult {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join("d"))?;
    fs::write(dir.path().join("f.rs"), "")?;
    let linked = symlink_dir(&dir.path().join("d"), &dir.path().join("l"));

    let mut kinds: Vec<(OsString, EntryKind)> = StdFiles
        .read_dir(dir.path())?
        .into_iter()
        .map(|entry| (entry.name, entry.kind))
        .collect();
    kinds.sort_by(|a, b| a.0.cmp(&b.0));

    let mut want = vec![
        (OsString::from("d"), EntryKind::Dir),
        (OsString::from("f.rs"), EntryKind::File),
    ];
    // Without the right to make links (Windows without developer mode) there is no link.
    if linked.is_ok() {
        want.push((OsString::from("l"), EntryKind::Symlink));
    }
    assert_eq!(kinds, want, "kinds from the entries, links not followed");
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore = "Miri cannot use the file system")]
fn real_path_resolves_symlinks_and_is_none_when_missing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let real_dir = StdFiles
        .real_path(dir.path())?
        .ok_or("the temporary directory exists")?;
    fs::create_dir(dir.path().join("d"))?;
    let linked = symlink_dir(&dir.path().join("d"), &dir.path().join("l"));

    let missing = StdFiles.real_path(&dir.path().join("missing"))?;
    let through_link = StdFiles.real_path(&dir.path().join("l"))?;

    assert_eq!(missing, None, "nothing there");
    if linked.is_ok() {
        assert_eq!(through_link, Some(real_dir.join("d")), "the link's target");
    }
    assert!(
        !real_dir.to_string_lossy().starts_with(r"\\?\"),
        "no verbatim prefix: {}",
        real_dir.display()
    );
    Ok(())
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
