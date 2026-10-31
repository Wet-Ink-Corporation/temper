//! `cargo xtask npm-pack X --bins DIR`: write the six platform packages (port plan §4 step 3).
//!
//! `DIR/<target triple>/temper[.exe]` holds each build job's binary. Each package lands in
//! `OUT/@wet-ink/temper-<os>-<cpu>/` with its `package.json`, both licence texts and
//! `bin/temper[.exe]` (mode 0755 where the file system has modes). Packing runs on Linux in the
//! release workflow, so the executable bit survives into the tarball.

use std::fs;
use std::path::{Path, PathBuf};

use crate::XtaskError;
use crate::npm::{TARGETS, package_json};

/// The licence files every package carries: (name in the package, root file).
const LICENCES: [(&str, &str); 2] = [
    ("LICENSE-MIT", "LICENSE"),
    ("LICENSE-APACHE", "LICENSE-APACHE"),
];

/// Write the six packages under `out` and return their directories, in [`TARGETS`] order.
pub(crate) fn pack(
    root: &Path,
    version: &str,
    bins: &Path,
    out: &Path,
) -> Result<Vec<PathBuf>, XtaskError> {
    // Every input is read before anything is written, so a missing binary or licence leaves no
    // half-written set of packages behind.
    let mut licences = Vec::with_capacity(LICENCES.len());
    for (name, at_root) in LICENCES {
        licences.push((name, read(&root.join(at_root))?));
    }
    let mut binaries = Vec::with_capacity(TARGETS.len());
    for target in TARGETS {
        let path = bins.join(target.triple).join(target.bin_name());
        if !path.is_file() {
            return Err(XtaskError::MissingBinary { path });
        }
        binaries.push((target, read(&path)?));
    }

    let mut written = Vec::with_capacity(TARGETS.len());
    for (target, binary) in binaries {
        let dir = out.join(target.package_name());
        // A file left from an earlier run must not ship in this one.
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|source| XtaskError::Write {
                path: dir.clone(),
                source,
            })?;
        }
        let bin_dir = dir.join("bin");
        create_dir(&bin_dir)?;
        write(
            &dir.join("package.json"),
            package_json(target, version).as_bytes(),
        )?;
        for (name, text) in &licences {
            write(&dir.join(name), text)?;
        }
        let installed = bin_dir.join(target.bin_name());
        write(&installed, &binary)?;
        // Windows has no mode bits; the package job runs on Linux, where they are set.
        #[cfg(unix)]
        make_executable(&installed)?;
        written.push(dir);
    }
    Ok(written)
}

fn read(path: &Path) -> Result<Vec<u8>, XtaskError> {
    fs::read(path).map_err(|source| XtaskError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir(path: &Path) -> Result<(), XtaskError> {
    fs::create_dir_all(path).map_err(|source| XtaskError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), XtaskError> {
    fs::write(path, bytes).map_err(|source| XtaskError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), XtaskError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|source| {
        XtaskError::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{LICENCES, pack};
    use crate::npm::TARGETS;
    use std::error::Error;
    use std::fs;
    use std::path::Path;

    type TestResult = Result<(), Box<dyn Error>>;

    fn fake_tree(root: &Path) -> TestResult {
        fs::write(root.join("LICENSE"), "MIT text")?;
        fs::write(root.join("LICENSE-APACHE"), "Apache text")?;
        for target in TARGETS {
            let dir = root.join("bins").join(target.triple);
            fs::create_dir_all(&dir)?;
            fs::write(dir.join(target.bin_name()), target.triple)?;
        }
        Ok(())
    }

    #[test]
    fn pack_writes_six_packages() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        fake_tree(root)?;
        let out = root.join("npm");
        let dirs = pack(root, "2.0.0-alpha.1", &root.join("bins"), &out)?;
        assert_eq!(dirs.len(), 6, "six packages");
        for (target, dir) in TARGETS.iter().zip(&dirs) {
            assert_eq!(dir, &out.join(target.package_name()), "package dir");
            let manifest = fs::read_to_string(dir.join("package.json"))?;
            assert!(
                manifest.contains("\"version\": \"2.0.0-alpha.1\""),
                "version in {manifest}"
            );
            assert!(!manifest.contains("\"scripts\""), "no lifecycle scripts");
            assert!(!manifest.contains("\"bin\":"), "no bin field");
            let binary = dir.join("bin").join(target.bin_name());
            assert_eq!(fs::read_to_string(&binary)?, target.triple, "binary copied");
            for (name, _) in LICENCES {
                assert!(dir.join(name).is_file(), "{name} in {}", dir.display());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&binary)?.permissions().mode() & 0o777,
                    0o755,
                    "mode of {}",
                    binary.display()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_missing_binary_is_an_error() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        fake_tree(root)?;
        let Some(last) = TARGETS.last() else {
            return Err("six targets".into());
        };
        fs::remove_file(root.join("bins").join(last.triple).join(last.bin_name()))?;
        let error = pack(root, "2.0.0", &root.join("bins"), &root.join("npm"))
            .err()
            .map(|e| e.to_string());
        assert!(
            error
                .as_deref()
                .is_some_and(|e| e.contains("aarch64-pc-windows-msvc")),
            "names the missing binary: {error:?}"
        );
        assert!(!root.join("npm").exists(), "nothing written");
        Ok(())
    }

    #[test]
    fn a_missing_root_licence_names_the_file_it_could_not_read() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        fake_tree(root)?;
        fs::remove_file(root.join("LICENSE-APACHE"))?;
        let error = pack(root, "2.0.0", &root.join("bins"), &root.join("npm"))
            .err()
            .map(|e| e.to_string());
        assert!(
            error
                .as_deref()
                .is_some_and(|e| e.starts_with("cannot read") && e.contains("LICENSE-APACHE")),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn a_stale_package_file_is_removed() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        fake_tree(root)?;
        let out = root.join("npm");
        let Some(first) = TARGETS.first() else {
            return Err("six targets".into());
        };
        let stale = out.join(first.package_name()).join("bin").join("old-file");
        fs::create_dir_all(stale.parent().ok_or("parent")?)?;
        fs::write(&stale, "from an earlier run")?;
        pack(root, "2.0.0", &root.join("bins"), &out)?;
        assert!(!stale.exists(), "an earlier run's file does not ship");
        Ok(())
    }
}
