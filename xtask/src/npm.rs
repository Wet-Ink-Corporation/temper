//! The npm side of a release: the six platform packages (`npm-pack`) and the lockfile check
//! (`check-npm-lock`, port plan invariant 6 and §4 steps 3 and 5).

use std::fmt;

/// The npm scope of the platform packages (D1).
pub(crate) const SCOPE: &str = "@wet-ink";

/// The registry every locked package must come from.
const REGISTRY: &str = "https://registry.npmjs.org/";

/// One release target and its npm package (D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    /// The Rust target triple the binary is built for.
    pub(crate) triple: &'static str,
    /// Node's `process.platform`.
    pub(crate) os: &'static str,
    /// Node's `process.arch`.
    pub(crate) cpu: &'static str,
}

/// The six release targets (D5).
pub(crate) const TARGETS: [Target; 6] = [
    Target {
        triple: "x86_64-unknown-linux-musl",
        os: "linux",
        cpu: "x64",
    },
    Target {
        triple: "aarch64-unknown-linux-musl",
        os: "linux",
        cpu: "arm64",
    },
    Target {
        triple: "x86_64-apple-darwin",
        os: "darwin",
        cpu: "x64",
    },
    Target {
        triple: "aarch64-apple-darwin",
        os: "darwin",
        cpu: "arm64",
    },
    Target {
        triple: "x86_64-pc-windows-msvc",
        os: "win32",
        cpu: "x64",
    },
    Target {
        triple: "aarch64-pc-windows-msvc",
        os: "win32",
        cpu: "arm64",
    },
];

impl Target {
    /// `@wet-ink/temper-<os>-<cpu>`.
    pub(crate) fn package_name(self) -> String {
        format!("{SCOPE}/temper-{}-{}", self.os, self.cpu)
    }

    /// The binary's file name: `temper.exe` on Windows, `temper` elsewhere.
    pub(crate) fn bin_name(self) -> &'static str {
        if self.os == "win32" {
            "temper.exe"
        } else {
            "temper"
        }
    }
}

/// The licence files every package carries: (name in the package, file at the repository root).
/// npm packs a `LICENSE-*` file only when `files` names it, so [`package_json`] lists each one.
pub(crate) const LICENCES: [(&str, &str); 2] = [
    ("LICENSE-MIT", "LICENSE"),
    ("LICENSE-APACHE", "LICENSE-APACHE"),
];

/// The `package.json` of `target`'s package at `version`: no `bin`, no `scripts` (§4 step 3).
/// Keys come out sorted, so the text is the same on every run.
pub(crate) fn package_json(target: Target, version: &str) -> String {
    let mut files = vec!["bin"];
    files.extend(LICENCES.map(|(name, _)| name));
    let value = serde_json::json!({
        "name": target.package_name(),
        "version": version,
        "description": format!("The temper engine for {} {}", target.os, target.cpu),
        "license": "MIT OR Apache-2.0",
        "repository": {
            "type": "git",
            "url": "git+https://github.com/Wet-Ink-Corporation/temper.git",
        },
        "os": [target.os],
        "cpu": [target.cpu],
        "files": files,
    });
    let mut text = format!("{value:#}");
    text.push('\n');
    text
}

/// Why `package.json` and `package-lock.json` would not install exactly one engine safely.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LockProblem {
    /// `package.json` has a key that runs code or rewrites resolution.
    ForbiddenKey(&'static str),
    /// A platform package is not pinned in `optionalDependencies`.
    MissingPin(String),
    /// A pin is a range, not an exact version.
    InexactPin { name: String, pin: String },
    /// The lockfile format predates per-package entries.
    OldLockfile(u64),
    /// A platform package has no entry in the lockfile (npm/cli#4828).
    MissingEntry(String),
    /// A locked package is not one of the six.
    ExtraPackage(String),
    /// A locked version differs from the pin.
    LockedVersion {
        name: String,
        locked: String,
        pin: String,
    },
    /// A locked package does not come from the npm registry over HTTPS.
    Resolved { name: String, resolved: String },
    /// A locked package has no sha512 integrity.
    Integrity(String),
    /// A locked package's `os` or `cpu` is not its target's, so npm would install the wrong engine.
    Platform {
        name: String,
        os: String,
        cpu: String,
        target: Target,
    },
    /// `optionalDependencies` names a package that is not one of the six.
    ExtraDependency(String),
}

/// What a lock entry's absent field is shown as.
const MISSING: &str = "<missing>";

impl fmt::Display for LockProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForbiddenKey(key) => write!(f, "package.json must not have \"{key}\""),
            Self::MissingPin(name) => {
                write!(f, "package.json optionalDependencies lacks {name}")
            }
            Self::InexactPin { name, pin } => {
                write!(
                    f,
                    "package.json pins {name} to \"{pin}\", not an exact version"
                )
            }
            Self::OldLockfile(version) => {
                write!(f, "package-lock.json has lockfileVersion {version}, need 3")
            }
            Self::MissingEntry(name) => write!(f, "package-lock.json has no entry for {name}"),
            Self::ExtraPackage(key) => write!(f, "package-lock.json has an extra package {key}"),
            Self::LockedVersion { name, locked, pin } => {
                write!(
                    f,
                    "package-lock.json locks {name} at {locked}, pinned {pin}"
                )
            }
            Self::Resolved { name, resolved } => {
                write!(f, "package-lock.json resolves {name} from {resolved}")
            }
            Self::Integrity(name) => {
                write!(f, "package-lock.json has no sha512 integrity for {name}")
            }
            Self::Platform {
                name,
                os,
                cpu,
                target,
            } => write!(
                f,
                "package-lock.json gives {name} os {os} and cpu {cpu}, expected [\"{}\"] and [\"{}\"]",
                target.os, target.cpu
            ),
            Self::ExtraDependency(name) => write!(
                f,
                "package.json optionalDependencies has {name}, which is not an engine package"
            ),
        }
    }
}

/// Every problem with the pair; empty when it is safe to `npm ci`.
pub(crate) fn lock_problems(
    package_json: &str,
    lock: &str,
) -> Result<Vec<LockProblem>, serde_json::Error> {
    let package: serde_json::Value = serde_json::from_str(package_json)?;
    let lock: serde_json::Value = serde_json::from_str(lock)?;
    let mut problems = Vec::new();
    let expected = package_problems(&package, &mut problems);

    // Absent reads as 0, which is reported like any other format but 3.
    let format = lock
        .get("lockfileVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if format != 3 {
        problems.push(LockProblem::OldLockfile(format));
        return Ok(problems);
    }
    let empty = serde_json::Map::new();
    let packages = lock
        .get("packages")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(&empty);
    for (target, name, pin) in &expected {
        let Some(entry) = packages.get(&format!("node_modules/{name}")) else {
            problems.push(LockProblem::MissingEntry(name.clone()));
            continue;
        };
        problems.extend(entry_problems(*target, name, pin, entry));
    }
    for key in packages.keys() {
        let known = key.is_empty()
            || expected
                .iter()
                .any(|(_, name, _)| key.strip_prefix("node_modules/") == Some(name.as_str()));
        if !known {
            problems.push(LockProblem::ExtraPackage(key.clone()));
        }
    }
    Ok(problems)
}

/// `package.json`'s problems, pushed onto `problems`; returns each pinned engine with its pin.
fn package_problems(
    package: &serde_json::Value,
    problems: &mut Vec<LockProblem>,
) -> Vec<(Target, String, String)> {
    // Scripts run code at install; overrides rewrite resolution; any other dependency field would
    // put packages besides the six engines in the tree.
    for key in [
        "scripts",
        "overrides",
        "dependencies",
        "devDependencies",
        "peerDependencies",
    ] {
        if package.get(key).is_some() {
            problems.push(LockProblem::ForbiddenKey(key));
        }
    }
    let pins = package
        .get("optionalDependencies")
        .and_then(serde_json::Value::as_object);
    let mut expected: Vec<(Target, String, String)> = Vec::new();
    for target in TARGETS {
        let name = target.package_name();
        match pins
            .and_then(|p| p.get(&name))
            .and_then(serde_json::Value::as_str)
        {
            None => problems.push(LockProblem::MissingPin(name)),
            Some(pin) => {
                if !is_exact_version(pin) {
                    problems.push(LockProblem::InexactPin {
                        name: name.clone(),
                        pin: pin.to_owned(),
                    });
                }
                expected.push((target, name, pin.to_owned()));
            }
        }
    }
    let engines: Vec<String> = TARGETS.iter().map(|t| t.package_name()).collect();
    for name in pins.into_iter().flat_map(serde_json::Map::keys) {
        if !engines.contains(name) {
            problems.push(LockProblem::ExtraDependency(name.clone()));
        }
    }
    expected
}

/// The problems with one platform package's lock entry.
fn entry_problems(
    target: Target,
    name: &str,
    pin: &str,
    entry: &serde_json::Value,
) -> Vec<LockProblem> {
    let field = |key| entry.get(key).and_then(serde_json::Value::as_str);
    let mut problems = Vec::new();
    let locked = field("version");
    if locked != Some(pin) {
        problems.push(LockProblem::LockedVersion {
            name: name.to_owned(),
            locked: locked.unwrap_or(MISSING).to_owned(),
            pin: pin.to_owned(),
        });
    }
    // The tarball must be this package's own, not any file on the registry.
    let resolved = field("resolved");
    if !resolved.is_some_and(|r| r.starts_with(&format!("{REGISTRY}{name}/-/"))) {
        problems.push(LockProblem::Resolved {
            name: name.to_owned(),
            resolved: resolved.unwrap_or(MISSING).to_owned(),
        });
    }
    if !field("integrity").is_some_and(|i| i.starts_with("sha512-")) {
        problems.push(LockProblem::Integrity(name.to_owned()));
    }
    let list = |key| {
        entry
            .get(key)
            .map_or_else(|| MISSING.to_owned(), ToString::to_string)
    };
    let only = |key, want: &str| {
        entry
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|values| values.len() == 1 && values.iter().all(|v| v == want))
    };
    if !only("os", target.os) || !only("cpu", target.cpu) {
        problems.push(LockProblem::Platform {
            name: name.to_owned(),
            os: list("os"),
            cpu: list("cpu"),
            target,
        });
    }
    problems
}

/// An exact semver version: `1.2.3`, optionally with `-pre` or `+build`; never a range.
pub(crate) fn is_exact_version(pin: &str) -> bool {
    let core = pin.split_once(['-', '+']).map_or(pin, |(core, _)| core);
    let mut parts = core.split('.');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    numeric(parts.next())
        && numeric(parts.next())
        && numeric(parts.next())
        && parts.next().is_none()
        && pin
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins(version: &str) -> String {
        TARGETS
            .iter()
            .map(|t| format!("    \"{}\": \"{version}\"", t.package_name()))
            .collect::<Vec<_>>()
            .join(",\n")
    }

    fn package(version: &str) -> String {
        format!(
            "{{\n  \"name\": \"temper-plugin\",\n  \"private\": true,\n  \"version\": \"{version}\",\n  \"optionalDependencies\": {{\n{}\n  }}\n}}\n",
            pins(version)
        )
    }

    fn entry(target: Target, version: &str) -> String {
        let name = target.package_name();
        format!(
            "    \"node_modules/{name}\": {{\n      \"version\": \"{version}\",\n      \"resolved\": \"https://registry.npmjs.org/{name}/-/temper-{}-{}-{version}.tgz\",\n      \"integrity\": \"sha512-abc\",\n      \"cpu\": [\"{cpu}\"],\n      \"optional\": true,\n      \"os\": [\"{os}\"]\n    }}",
            target.os,
            target.cpu,
            os = target.os,
            cpu = target.cpu
        )
    }

    fn lock(version: &str, entries: &[String]) -> String {
        let mut packages = vec![format!(
            "    \"\": {{\n      \"name\": \"temper-plugin\",\n      \"version\": \"{version}\"\n    }}"
        )];
        packages.extend(entries.iter().cloned());
        format!(
            "{{\n  \"name\": \"temper-plugin\",\n  \"lockfileVersion\": 3,\n  \"packages\": {{\n{}\n  }}\n}}\n",
            packages.join(",\n")
        )
    }

    fn good_entries(version: &str) -> Vec<String> {
        TARGETS.iter().map(|t| entry(*t, version)).collect()
    }

    fn problems(package_json: &str, lock: &str) -> Vec<String> {
        match lock_problems(package_json, lock) {
            Ok(found) => found.iter().map(ToString::to_string).collect(),
            Err(error) => vec![format!("JSON error: {error}")],
        }
    }

    #[test]
    fn targets_name_their_packages_and_binaries() {
        let names: Vec<(String, &str)> = TARGETS
            .iter()
            .map(|t| (t.package_name(), t.bin_name()))
            .collect();
        assert_eq!(
            names,
            [
                ("@wet-ink/temper-linux-x64".to_owned(), "temper"),
                ("@wet-ink/temper-linux-arm64".to_owned(), "temper"),
                ("@wet-ink/temper-darwin-x64".to_owned(), "temper"),
                ("@wet-ink/temper-darwin-arm64".to_owned(), "temper"),
                ("@wet-ink/temper-win32-x64".to_owned(), "temper.exe"),
                ("@wet-ink/temper-win32-arm64".to_owned(), "temper.exe"),
            ],
            "the six packages"
        );
    }

    #[test]
    fn package_json_is_exact() {
        let expected = r#"{
  "cpu": [
    "arm64"
  ],
  "description": "The temper engine for win32 arm64",
  "files": [
    "bin",
    "LICENSE-MIT",
    "LICENSE-APACHE"
  ],
  "license": "MIT OR Apache-2.0",
  "name": "@wet-ink/temper-win32-arm64",
  "os": [
    "win32"
  ],
  "repository": {
    "type": "git",
    "url": "git+https://github.com/Wet-Ink-Corporation/temper.git"
  },
  "version": "2.0.0-alpha.1"
}
"#;
        let Some(target) = TARGETS.get(5) else {
            panic!("six targets");
        };
        assert_eq!(
            package_json(*target, "2.0.0-alpha.1"),
            expected,
            "package.json"
        );
    }

    #[test]
    fn a_good_pair_has_no_problem() {
        assert_eq!(
            problems(&package("2.0.0"), &lock("2.0.0", &good_entries("2.0.0"))),
            Vec::<String>::new(),
            "good pair"
        );
    }

    #[test]
    fn a_missing_platform_entry_is_reported() {
        let mut entries = good_entries("2.0.0");
        entries.remove(3);
        assert_eq!(
            problems(&package("2.0.0"), &lock("2.0.0", &entries)),
            ["package-lock.json has no entry for @wet-ink/temper-darwin-arm64"],
            "the npm/cli#4828 shape"
        );
    }

    #[test]
    fn a_range_pin_is_reported() {
        let package = package("2.0.0").replacen(
            "\"@wet-ink/temper-linux-x64\": \"2.0.0\"",
            "\"@wet-ink/temper-linux-x64\": \"^2.0.0\"",
            1,
        );
        assert_eq!(
            problems(&package, &lock("2.0.0", &good_entries("2.0.0"))),
            [
                "package.json pins @wet-ink/temper-linux-x64 to \"^2.0.0\", not an exact version",
                "package-lock.json locks @wet-ink/temper-linux-x64 at 2.0.0, pinned ^2.0.0",
            ],
            "non-exact pin"
        );
    }

    #[test]
    fn an_http_resolved_url_is_reported() {
        let lock = lock("2.0.0", &good_entries("2.0.0")).replacen(
            "https://registry.npmjs.org/@wet-ink/temper-win32-x64",
            "http://registry.npmjs.org/@wet-ink/temper-win32-x64",
            1,
        );
        assert_eq!(
            problems(&package("2.0.0"), &lock),
            [
                "package-lock.json resolves @wet-ink/temper-win32-x64 from http://registry.npmjs.org/@wet-ink/temper-win32-x64/-/temper-win32-x64-2.0.0.tgz"
            ],
            "http"
        );
    }

    #[test]
    fn a_missing_integrity_is_reported() {
        let mut entries = good_entries("2.0.0");
        if let Some(first) = entries.first_mut() {
            *first = first.replace("      \"integrity\": \"sha512-abc\",\n", "");
        }
        assert_eq!(
            problems(&package("2.0.0"), &lock("2.0.0", &entries)),
            ["package-lock.json has no sha512 integrity for @wet-ink/temper-linux-x64"],
            "integrity"
        );
    }

    #[test]
    fn an_extra_package_is_reported() {
        let mut entries = good_entries("2.0.0");
        entries.push(
            "    \"node_modules/left-pad\": {\n      \"version\": \"1.3.0\",\n      \"resolved\": \"https://registry.npmjs.org/left-pad/-/left-pad-1.3.0.tgz\",\n      \"integrity\": \"sha512-x\"\n    }".to_owned(),
        );
        assert_eq!(
            problems(&package("2.0.0"), &lock("2.0.0", &entries)),
            ["package-lock.json has an extra package node_modules/left-pad"],
            "extra"
        );
    }

    #[test]
    fn overrides_and_scripts_are_reported() {
        let package = package("2.0.0").replacen(
            "  \"private\": true,\n",
            "  \"private\": true,\n  \"scripts\": {\"postinstall\": \"x\"},\n  \"overrides\": {},\n",
            1,
        );
        assert_eq!(
            problems(&package, &lock("2.0.0", &good_entries("2.0.0"))),
            [
                "package.json must not have \"scripts\"",
                "package.json must not have \"overrides\"",
            ],
            "forbidden keys"
        );
    }

    #[test]
    fn a_lockfile_version_other_than_3_is_reported() {
        for old in [1, 2] {
            let lock = lock("2.0.0", &good_entries("2.0.0")).replacen(
                "\"lockfileVersion\": 3",
                &format!("\"lockfileVersion\": {old}"),
                1,
            );
            assert_eq!(
                problems(&package("2.0.0"), &lock),
                [format!(
                    "package-lock.json has lockfileVersion {old}, need 3"
                )],
                "lockfile {old}"
            );
        }
    }

    #[test]
    fn a_wrong_platform_is_reported() {
        let lock = lock("2.0.0", &good_entries("2.0.0")).replacen(
            "temper-linux-arm64-2.0.0.tgz\",\n      \"integrity\": \"sha512-abc\",\n      \"cpu\": [\"arm64\"]",
            "temper-linux-arm64-2.0.0.tgz\",\n      \"integrity\": \"sha512-abc\",\n      \"cpu\": [\"x64\"]",
            1,
        );
        assert_eq!(
            problems(&package("2.0.0"), &lock),
            [
                "package-lock.json gives @wet-ink/temper-linux-arm64 os [\"linux\"] and cpu [\"x64\"], expected [\"linux\"] and [\"arm64\"]"
            ],
            "wrong cpu"
        );
    }

    #[test]
    fn a_tarball_of_another_package_is_reported() {
        let lock = lock("2.0.0", &good_entries("2.0.0")).replacen(
            "https://registry.npmjs.org/@wet-ink/temper-darwin-x64/-/",
            "https://registry.npmjs.org/evil/-/",
            1,
        );
        assert_eq!(
            problems(&package("2.0.0"), &lock),
            [
                "package-lock.json resolves @wet-ink/temper-darwin-x64 from https://registry.npmjs.org/evil/-/temper-darwin-x64-2.0.0.tgz"
            ],
            "another package's tarball"
        );
    }

    #[test]
    fn a_missing_version_field_says_so() {
        let mut entries = good_entries("2.0.0");
        if let Some(first) = entries.first_mut() {
            *first = first.replace("      \"version\": \"2.0.0\",\n", "");
        }
        assert_eq!(
            problems(&package("2.0.0"), &lock("2.0.0", &entries)),
            ["package-lock.json locks @wet-ink/temper-linux-x64 at <missing>, pinned 2.0.0"],
            "absent field named"
        );
    }

    #[test]
    fn other_dependencies_are_reported() {
        let package = package("2.0.0")
            .replacen(
                "  \"private\": true,\n",
                "  \"private\": true,\n  \"dependencies\": {},\n",
                1,
            )
            .replacen(
                "\"@wet-ink/temper-linux-x64\": \"2.0.0\"",
                "\"left-pad\": \"1.3.0\",\n    \"@wet-ink/temper-linux-x64\": \"2.0.0\"",
                1,
            );
        assert_eq!(
            problems(&package, &lock("2.0.0", &good_entries("2.0.0"))),
            [
                "package.json must not have \"dependencies\"",
                "package.json optionalDependencies has left-pad, which is not an engine package",
            ],
            "non-engine dependencies"
        );
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(lock_problems("{", "{}").is_err(), "package.json");
        assert!(lock_problems("{}", "[").is_err(), "lockfile");
    }
}
