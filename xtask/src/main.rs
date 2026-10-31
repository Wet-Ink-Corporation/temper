//! `cargo xtask <task>`: temper's repository tooling. Never published.
//!
//! Tasks: `check-licenses`, `versions --check [X]`, `versions --set X`, `check-npm-lock`,
//! `npm-pack X --bins DIR`.

mod licenses;
mod npm;
mod pack;
mod versions;

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use licenses::{COPIES, Problem};
use versions::Site;

const USAGE: &str = "usage: cargo xtask <task>
  check-licenses           every crate's LICENSE-MIT and LICENSE-APACHE equal the root's
  versions --check [X]     every version site agrees (and equals X)
  versions --set X         set every version site to X
  check-npm-lock           package.json and package-lock.json install one engine safely
  npm-pack X --bins DIR    write target/npm/@wet-ink/temper-*/ from DIR/<target>/temper[.exe]";

/// One command line, understood.
#[derive(Debug, PartialEq, Eq)]
enum Task {
    CheckLicenses,
    VersionsCheck(Option<String>),
    VersionsSet(String),
    CheckNpmLock,
    NpmPack { version: String, bins: PathBuf },
}

/// The task the arguments ask for, or `None` for a usage error (including non-UTF-8).
fn parse_task(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<Task> {
    let args: Vec<String> = args
        .into_iter()
        .map(std::ffi::OsString::into_string)
        .collect::<Result<_, _>>()
        .ok()?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["check-licenses"] => Some(Task::CheckLicenses),
        ["versions", "--check"] => Some(Task::VersionsCheck(None)),
        ["versions", "--check", version] => Some(Task::VersionsCheck(Some((*version).to_owned()))),
        ["versions", "--set", version] => Some(Task::VersionsSet((*version).to_owned())),
        ["check-npm-lock"] => Some(Task::CheckNpmLock),
        ["npm-pack", version, "--bins", bins] => Some(Task::NpmPack {
            version: (*version).to_owned(),
            bins: PathBuf::from(bins),
        }),
        _ => None,
    }
}

/// What a task found: the lines to print, and whether it passed.
struct Outcome {
    lines: Vec<String>,
    passed: bool,
}

impl Outcome {
    fn pass(line: impl Into<String>) -> Self {
        Self {
            lines: vec![line.into()],
            passed: true,
        }
    }

    fn fail(lines: Vec<String>) -> Self {
        Self {
            lines,
            passed: false,
        }
    }
}

fn main() -> ExitCode {
    let Some(task) = parse_task(std::env::args_os().skip(1)) else {
        return report(&[USAGE], Exit::Usage);
    };
    let root = workspace_root();
    let (name, outcome) = match task {
        Task::CheckLicenses => ("check-licenses", check_licenses(&root)),
        Task::VersionsCheck(expected) => ("versions", versions_check(&root, expected.as_deref())),
        Task::VersionsSet(version) => ("versions", versions_set(&root, &version)),
        Task::CheckNpmLock => ("check-npm-lock", check_npm_lock(&root)),
        Task::NpmPack { version, bins } => ("npm-pack", npm_pack(&root, &version, &bins)),
    };
    match outcome {
        Ok(Outcome { lines, passed }) => {
            let lines: Vec<String> = lines.iter().map(|l| format!("{name}: {l}")).collect();
            report(&lines, if passed { Exit::Success } else { Exit::Failure })
        }
        Err(error) => report(&[format!("{name}: {error}")], Exit::Failure),
    }
}

/// The workspace root: the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .map_or_else(|| here.to_path_buf(), Path::to_path_buf)
}

fn check_licenses(root: &Path) -> Result<Outcome, XtaskError> {
    let problems = licence_problems(root)?;
    if problems.is_empty() {
        return Ok(Outcome::pass("every crate's licence copies match the root"));
    }
    Ok(Outcome::fail(
        problems.iter().map(ToString::to_string).collect(),
    ))
}

/// Every licence copy under `crates/*` that differs from the root, or is missing.
fn licence_problems(root: &Path) -> Result<Vec<Problem>, XtaskError> {
    let crates_dir = root.join("crates");
    let unreadable = |source| XtaskError::Read {
        path: crates_dir.clone(),
        source,
    };
    let mut crates: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .map_err(unreadable)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(unreadable)?;
    crates.retain(|dir| dir.join("Cargo.toml").is_file());
    crates.sort();
    let mut problems = Vec::new();
    for copy in COPIES {
        let root_text = read(&root.join(copy.at_root))?;
        for dir in &crates {
            let name = dir
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let path = dir.join(copy.in_crate);
            let found = if path.is_file() {
                Some(read(&path)?)
            } else {
                None
            };
            problems.extend(licenses::compare(&name, copy, &root_text, found.as_deref()));
        }
    }
    Ok(problems)
}

/// The files that hold versions, relative to the root.
mod site_files {
    pub(crate) const CARGO_TOML: &str = "Cargo.toml";
    pub(crate) const CARGO_LOCK: &str = "Cargo.lock";
    pub(crate) const PACKAGE_JSON: &str = "package.json";
    pub(crate) const PLUGIN_JSON: &str = ".claude-plugin/plugin.json";
    pub(crate) const TEMPER_VERSIONS: [&str; 2] = [
        "templates/workspace/.claude/temper-version",
        "templates/example-workspace/.claude/temper-version",
    ];
}

/// Every version site that exists, in a fixed order.
fn read_sites(root: &Path) -> Result<Vec<Site>, XtaskError> {
    let mut sites = versions::cargo_sites(&read(&root.join(site_files::CARGO_TOML))?);
    sites.extend(versions::lock_sites(&read(
        &root.join(site_files::CARGO_LOCK),
    )?));
    let package = root.join(site_files::PACKAGE_JSON);
    if package.is_file() {
        sites.extend(versions::package_json_sites(&read(&package)?).map_err(json(&package))?);
        let plugin = root.join(site_files::PLUGIN_JSON);
        if let Some(version) = versions::json_version(&read(&plugin)?).map_err(json(&plugin))? {
            sites.push(Site {
                name: site_files::PLUGIN_JSON.to_owned(),
                version,
            });
        }
    }
    for file in site_files::TEMPER_VERSIONS {
        let path = root.join(file);
        if path.is_file() {
            sites.push(Site {
                name: file.to_owned(),
                version: read(&path)?.trim().to_owned(),
            });
        }
    }
    Ok(sites)
}

fn versions_check(root: &Path, expected: Option<&str>) -> Result<Outcome, XtaskError> {
    let sites = read_sites(root)?;
    let mismatches = versions::mismatches(&sites, expected);
    if !mismatches.is_empty() {
        return Ok(Outcome::fail(
            mismatches.iter().map(ToString::to_string).collect(),
        ));
    }
    let version = sites.first().map_or("none", |s| s.version.as_str());
    Ok(Outcome::pass(format!(
        "{} sites agree on {version}",
        sites.len()
    )))
}

fn versions_set(root: &Path, version: &str) -> Result<Outcome, XtaskError> {
    if !versions::is_prerelease(version) {
        let changelog = read(&root.join("CHANGELOG.md"))?;
        if !versions::has_changelog_heading(&changelog, version) {
            return Ok(Outcome::fail(vec![format!(
                "CHANGELOG.md has no \"## {version}\" heading; nothing was changed"
            )]));
        }
    }
    // Every new text is computed before anything is written, so a read failure changes nothing.
    let mut writes = Vec::new();
    let manifest = root.join(site_files::CARGO_TOML);
    let text = versions::set_cargo_versions(&read(&manifest)?, version);
    writes.push((manifest, text));
    let lock = root.join(site_files::CARGO_LOCK);
    let text = versions::set_lock_versions(&read(&lock)?, version);
    writes.push((lock, text));
    let package = root.join(site_files::PACKAGE_JSON);
    if package.is_file() {
        let names: Vec<String> = npm::TARGETS.iter().map(|t| t.package_name()).collect();
        let mut keys = vec!["version"];
        keys.extend(names.iter().map(String::as_str));
        let text = versions::set_json_strings(&read(&package)?, &keys, version);
        writes.push((package, text));
        let plugin = root.join(site_files::PLUGIN_JSON);
        let text = versions::set_json_strings(&read(&plugin)?, &["version"], version);
        writes.push((plugin, text));
    }
    for file in site_files::TEMPER_VERSIONS {
        let path = root.join(file);
        if path.is_file() {
            writes.push((path, format!("{version}\n")));
        }
    }
    for (path, text) in writes {
        if let Err(source) = fs::write(&path, text) {
            return Err(XtaskError::Write { path, source });
        }
    }
    versions_check(root, Some(version))
}

fn check_npm_lock(root: &Path) -> Result<Outcome, XtaskError> {
    let package = root.join(site_files::PACKAGE_JSON);
    let lock = root.join("package-lock.json");
    let problems = npm::lock_problems(&read(&package)?, &read(&lock)?).map_err(json(&lock))?;
    if problems.is_empty() {
        return Ok(Outcome::pass(
            "package.json and package-lock.json pin the six engines exactly",
        ));
    }
    Ok(Outcome::fail(
        problems.iter().map(ToString::to_string).collect(),
    ))
}

fn npm_pack(root: &Path, version: &str, bins: &Path) -> Result<Outcome, XtaskError> {
    let out = root.join("target").join("npm");
    let dirs = pack::pack(root, version, bins, &out)?;
    Ok(Outcome::pass(format!(
        "wrote {} packages under {}",
        dirs.len(),
        out.display()
    )))
}

fn read(path: &Path) -> Result<String, XtaskError> {
    fs::read_to_string(path).map_err(|source| XtaskError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Map a JSON error to [`XtaskError::Json`] for `path`.
fn json(path: &Path) -> impl FnOnce(serde_json::Error) -> XtaskError + '_ {
    move |source| XtaskError::Json {
        path: path.to_path_buf(),
        source,
    }
}

/// Write each line to stdout (success) or stderr (anything else), then exit.
fn report(lines: &[impl fmt::Display], exit: Exit) -> ExitCode {
    let written = match exit {
        Exit::Success => write_lines(io::stdout().lock(), lines),
        Exit::Failure | Exit::Usage => write_lines(io::stderr().lock(), lines),
    };
    match written {
        Ok(()) => exit.into(),
        Err(_) => Exit::Failure.into(),
    }
}

fn write_lines(mut out: impl Write, lines: &[impl fmt::Display]) -> io::Result<()> {
    for line in lines {
        writeln!(out, "{line}")?;
    }
    out.flush()
}

/// Why a task could not run.
#[derive(Debug)]
enum XtaskError {
    /// A file or directory could not be read.
    Read { path: PathBuf, source: io::Error },
    /// A file or directory could not be written.
    Write { path: PathBuf, source: io::Error },
    /// A JSON file did not parse.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// A build job's binary is not where `npm-pack` looks for it.
    MissingBinary { path: PathBuf },
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Write { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
            Self::Json { path, source } => {
                write!(f, "{} is not valid JSON: {source}", path.display())
            }
            Self::MissingBinary { path } => write!(f, "no binary at {}", path.display()),
        }
    }
}

/// `Display` already names the underlying error, so `source` is not repeated (ERR-05).
impl std::error::Error for XtaskError {}

/// The exit codes: 0 done, 1 a check failed or a file could not be read, 2 usage.
enum Exit {
    Success,
    Failure,
    Usage,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => Self::SUCCESS,
            Exit::Failure => Self::from(1),
            Exit::Usage => Self::from(2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Task, parse_task, read_sites, versions_check, versions_set};
    use std::error::Error;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    type TestResult = Result<(), Box<dyn Error>>;

    fn parse(args: &[&str]) -> Option<Task> {
        parse_task(args.iter().map(OsString::from))
    }

    #[test]
    fn tasks_parse() {
        assert_eq!(
            parse(&["check-licenses"]),
            Some(Task::CheckLicenses),
            "licences"
        );
        assert_eq!(
            parse(&["versions", "--check"]),
            Some(Task::VersionsCheck(None)),
            "check"
        );
        assert_eq!(
            parse(&["versions", "--check", "2.0.0"]),
            Some(Task::VersionsCheck(Some("2.0.0".to_owned()))),
            "check X"
        );
        assert_eq!(
            parse(&["versions", "--set", "2.0.0"]),
            Some(Task::VersionsSet("2.0.0".to_owned())),
            "set"
        );
        assert_eq!(parse(&["check-npm-lock"]), Some(Task::CheckNpmLock), "lock");
        assert_eq!(
            parse(&["npm-pack", "2.0.0", "--bins", "dist"]),
            Some(Task::NpmPack {
                version: "2.0.0".to_owned(),
                bins: PathBuf::from("dist")
            }),
            "pack"
        );
    }

    #[test]
    fn usage_errors_parse_to_none() {
        for args in [
            &[][..],
            &["bogus"],
            &["versions"],
            &["versions", "--set"],
            &["npm-pack", "2.0.0"],
            &["check-licenses", "x"],
        ] {
            assert_eq!(parse(args), None, "{args:?}");
        }
    }

    const MANIFEST: &str = "[workspace.package]\nversion = \"2.0.0-alpha.0\"\n\n[workspace.dependencies]\ntemper-app = { path = \"crates/temper-app\", version = \"2.0.0-alpha.0\" }\n";
    const LOCK: &str =
        "version = 4\n\n[[package]]\nname = \"temper-app\"\nversion = \"2.0.0-alpha.0\"\n";

    fn tree(root: &Path) -> TestResult {
        fs::write(root.join("Cargo.toml"), MANIFEST)?;
        fs::write(root.join("Cargo.lock"), LOCK)?;
        fs::create_dir_all(root.join(".claude-plugin"))?;
        fs::write(
            root.join(".claude-plugin/plugin.json"),
            "{\n  \"name\": \"temper\",\n  \"version\": \"1.5.0\"\n}\n",
        )?;
        fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\n## 1.5.0 (2026-10-31)\n",
        )?;
        Ok(())
    }

    fn site_names(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
        Ok(read_sites(root)?
            .into_iter()
            .map(|s| format!("{} {}", s.name, s.version))
            .collect())
    }

    #[test]
    fn plugin_json_is_a_site_only_once_package_json_exists() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        tree(root)?;
        assert_eq!(
            site_names(root)?,
            [
                "Cargo.toml [workspace.package] 2.0.0-alpha.0",
                "Cargo.toml [workspace.dependencies] temper-app 2.0.0-alpha.0",
                "Cargo.lock temper-app 2.0.0-alpha.0",
            ],
            "during the port the 1.x plugin.json is not a site"
        );
        assert!(versions_check(root, None)?.passed, "they agree");

        fs::write(
            root.join("package.json"),
            "{\n  \"version\": \"2.0.0-alpha.0\",\n  \"optionalDependencies\": {\n    \"@wet-ink/temper-linux-x64\": \"2.0.0-alpha.0\"\n  }\n}\n",
        )?;
        let outcome = versions_check(root, None)?;
        assert!(!outcome.passed, "plugin.json now disagrees");
        assert_eq!(
            outcome.lines,
            [".claude-plugin/plugin.json is 1.5.0, expected 2.0.0-alpha.0"],
            "names the site"
        );
        Ok(())
    }

    #[test]
    fn each_disagreeing_site_fails_the_check() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        tree(root)?;
        let template = root.join("templates/workspace/.claude");
        fs::create_dir_all(&template)?;
        fs::write(template.join("temper-version"), "2.0.0-alpha.0\n")?;
        assert!(
            versions_check(root, Some("2.0.0-alpha.0"))?.passed,
            "all agree"
        );
        assert!(!versions_check(root, Some("2.0.0"))?.passed, "not X");

        for (file, from, to) in [
            (
                "Cargo.toml",
                "version = \"2.0.0-alpha.0\"\n\n",
                "version = \"2.0.0-alpha.9\"\n\n",
            ),
            (
                "Cargo.toml",
                "temper-app\", version = \"2.0.0-alpha.0\"",
                "temper-app\", version = \"2.0.0-alpha.9\"",
            ),
            ("Cargo.lock", "\"2.0.0-alpha.0\"", "\"2.0.0-alpha.9\""),
            (
                "templates/workspace/.claude/temper-version",
                "alpha.0",
                "alpha.9",
            ),
        ] {
            let path = root.join(file);
            let original = fs::read_to_string(&path)?;
            fs::write(&path, original.replacen(from, to, 1))?;
            let outcome = versions_check(root, Some("2.0.0-alpha.0"))?;
            assert_eq!(
                outcome.lines.len(),
                1,
                "one mismatch for {file}: {:?}",
                outcome.lines
            );
            assert!(!outcome.passed, "{file}");
            fs::write(&path, original)?;
        }
        Ok(())
    }

    #[test]
    fn set_rewrites_every_site_and_a_release_needs_a_changelog_heading() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        tree(root)?;
        let outcome = versions_set(root, "2.0.0")?;
        assert!(!outcome.passed, "no ## 2.0.0 heading");
        assert!(
            fs::read_to_string(root.join("Cargo.toml"))?.contains("2.0.0-alpha.0"),
            "nothing written"
        );

        let outcome = versions_set(root, "2.0.0-alpha.1")?;
        assert!(
            outcome.passed,
            "a pre-release needs no heading: {:?}",
            outcome.lines
        );
        assert_eq!(
            outcome.lines,
            ["3 sites agree on 2.0.0-alpha.1"],
            "every site set"
        );
        assert!(
            fs::read_to_string(root.join(".claude-plugin/plugin.json"))?.contains("1.5.0"),
            "plugin.json untouched during the port"
        );
        Ok(())
    }
}
