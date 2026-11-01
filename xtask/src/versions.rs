//! `cargo xtask versions`: the version sites of a release agree (port plan §4, "Versioning").
//!
//! The sites, while the port is in progress:
//! - `Cargo.toml`: `[workspace.package] version` and each sibling's `version` in
//!   `[workspace.dependencies]`;
//! - `Cargo.lock`: the workspace members' entries;
//! - `package.json`: `version` and the six platform pins, once the file exists;
//! - `.claude-plugin/plugin.json`: only once `package.json` exists. Until the cutover the plugin
//!   ships 1.x while the workspace carries a 2.0 pre-release, by design;
//! - `plugins/temper-lens/.claude-plugin/plugin.json`: under the same condition, for the same
//!   reason;
//! - `templates/{workspace,example-workspace}/.claude/temper-version`, once present (Phase 7).
//!
//! Everything here is text in, text out; `main.rs` reads and writes the files.

use std::fmt;

use crate::npm::TARGETS;

/// One place that records a version, and the version it records.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Site {
    /// Where the version is, such as `Cargo.toml [workspace.package]`.
    pub(crate) name: String,
    /// The version found there.
    pub(crate) version: String,
}

impl Site {
    fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// A site that does not record the expected version.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Mismatch<'a> {
    /// The site.
    pub(crate) site: &'a Site,
    /// The version every site should record.
    pub(crate) expected: &'a str,
}

impl fmt::Display for Mismatch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let found = if self.site.version.is_empty() {
            "missing"
        } else {
            &self.site.version
        };
        write!(
            f,
            "{} is {found}, expected {}",
            self.site.name, self.expected
        )
    }
}

/// Every site whose version differs from `expected`, or, without one, from the first site's.
pub(crate) fn mismatches<'a>(sites: &'a [Site], expected: Option<&'a str>) -> Vec<Mismatch<'a>> {
    let Some(expected) = expected.or_else(|| sites.first().map(|s| s.version.as_str())) else {
        return Vec::new();
    };
    sites
        .iter()
        .filter(|site| site.version != expected)
        .map(|site| Mismatch { site, expected })
        .collect()
}

/// The `[workspace.package]` version and each sibling's version in `[workspace.dependencies]`,
/// in file order.
pub(crate) fn cargo_sites(manifest: &str) -> Vec<Site> {
    let mut sites = Vec::new();
    let mut section = "";
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
            continue;
        }
        match section {
            "[workspace.package]" => {
                if let Some(version) = assigned_string(trimmed, "version") {
                    sites.push(Site::new("Cargo.toml [workspace.package]", version));
                }
            }
            "[workspace.dependencies]" => {
                if let Some((name, version)) = sibling_version(trimmed) {
                    sites.push(Site::new(
                        format!("Cargo.toml [workspace.dependencies] {name}"),
                        version,
                    ));
                }
            }
            _ => {}
        }
    }
    sites
}

/// `manifest` with every version [`cargo_sites`] reads set to `to`; nothing else changes.
pub(crate) fn set_cargo_versions(manifest: &str, to: &str) -> String {
    let mut out = String::with_capacity(manifest.len());
    let mut section = "";
    for line in manifest.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
            out.push_str(line);
            continue;
        }
        let old = match section {
            "[workspace.package]" => assigned_string(trimmed, "version"),
            "[workspace.dependencies]" => sibling_version(trimmed).map(|(_, version)| version),
            _ => None,
        };
        match old {
            Some(old) => out.push_str(&replace_quoted(line, old, to)),
            None => out.push_str(line),
        }
    }
    out
}

/// The workspace members' entries in `Cargo.lock`: the packages without a `source`.
pub(crate) fn lock_sites(lock: &str) -> Vec<Site> {
    lock_packages(lock)
        .into_iter()
        .filter(|p| !p.has_source)
        .filter_map(|p| {
            let version = p.version?;
            Some(Site::new(format!("Cargo.lock {}", p.name), version))
        })
        .collect()
}

/// `lock` with every workspace member's version set to `to`.
pub(crate) fn set_lock_versions(lock: &str, to: &str) -> String {
    let local: Vec<String> = lock_packages(lock)
        .into_iter()
        .filter(|p| !p.has_source)
        .map(|p| p.name)
        .collect();
    let mut out = String::with_capacity(lock.len());
    let mut current: Option<String> = None;
    for line in lock.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            current = None;
        } else if let Some(name) = assigned_string(trimmed, "name") {
            current = Some(name.to_owned());
        } else if let (Some(name), Some(old)) = (&current, assigned_string(trimmed, "version"))
            && local.contains(name)
        {
            out.push_str(&replace_quoted(line, old, to));
            continue;
        }
        out.push_str(line);
    }
    out
}

/// The top-level `version` of a JSON document such as `plugin.json`.
pub(crate) fn json_version(text: &str) -> Result<Option<String>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

/// `package.json`'s `version` and each `@wet-ink/temper-*` pin in `optionalDependencies`.
/// An absent version or pin is a site with an empty version, so the check reports it as missing.
pub(crate) fn package_json_sites(text: &str) -> Result<Vec<Site>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let string = |v: Option<&serde_json::Value>| {
        v.and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut sites = vec![Site::new(
        "package.json version",
        string(value.get("version")),
    )];
    let pins = value.get("optionalDependencies");
    for target in TARGETS {
        let name = target.package_name();
        let pin = string(pins.and_then(|p| p.get(&name)));
        sites.push(Site::new(format!("package.json {name}"), pin));
    }
    Ok(sites)
}

/// `text` with the string value of each JSON `key` replaced by `to`, keeping the formatting.
/// Used for `version` and the platform pins, whose keys appear once each.
pub(crate) fn set_json_strings(text: &str, keys: &[&str], to: &str) -> String {
    let mut out = text.to_owned();
    for key in keys {
        let needle = format!("\"{key}\"");
        let Some(at) = out.find(&needle) else {
            continue;
        };
        let after_key = at + needle.len();
        let Some(rest) = out.get(after_key..) else {
            continue;
        };
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let Some(value_rest) = rest.get(colon + 1..) else {
            continue;
        };
        let Some(open) = value_rest.find('"') else {
            continue;
        };
        let start = after_key + colon + 1 + open + 1;
        let Some(len) = out.get(start..).and_then(|s| s.find('"')) else {
            continue;
        };
        out.replace_range(start..start + len, to);
    }
    out
}

/// Whether `version` is a pre-release (`2.0.0-alpha.1`), which needs no CHANGELOG heading.
pub(crate) fn is_prerelease(version: &str) -> bool {
    version.contains('-')
}

/// Whether `changelog` has a `## <version>` heading (`## 2.0.0 (2026-11-01)` counts).
pub(crate) fn has_changelog_heading(changelog: &str, version: &str) -> bool {
    changelog.lines().any(|line| {
        line.strip_prefix("## ")
            .and_then(|rest| rest.split_whitespace().next())
            == Some(version)
    })
}

struct LockPackage {
    name: String,
    version: Option<String>,
    has_source: bool,
}

fn lock_packages(lock: &str) -> Vec<LockPackage> {
    let mut packages: Vec<LockPackage> = Vec::new();
    for line in lock.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            packages.push(LockPackage {
                name: String::new(),
                version: None,
                has_source: false,
            });
            continue;
        }
        let Some(package) = packages.last_mut() else {
            continue;
        };
        if let Some(name) = assigned_string(trimmed, "name") {
            name.clone_into(&mut package.name);
        } else if let Some(version) = assigned_string(trimmed, "version") {
            package.version = Some(version.to_owned());
        } else if trimmed.starts_with("source") {
            package.has_source = true;
        }
    }
    packages
}

/// The string in `key = "value"`, or `None` when the line assigns something else.
fn assigned_string<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    rest.split('"').next()
}

/// `temper-x = { path = "..", version = "1.2.3" }` gives `("temper-x", "1.2.3")`.
fn sibling_version(line: &str) -> Option<(&str, &str)> {
    let (name, rest) = line.split_once('=')?;
    let name = name.trim();
    if !name.starts_with("temper-") {
        return None;
    }
    let (_, after) = rest.split_once("version")?;
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let after = after.strip_prefix('"')?;
    Some((name, after.split('"').next()?))
}

/// `line` with the first `"old"` replaced by `"new"`.
fn replace_quoted(line: &str, old: &str, new: &str) -> String {
    line.replacen(&format!("\"{old}\""), &format!("\"{new}\""), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"[workspace]
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "2.0.0-alpha.0"      # comment
license = "MIT OR Apache-2.0"

[workspace.dependencies]
temper-domain = { path = "crates/temper-domain", version = "2.0.0-alpha.0" }
temper-app = { path = "crates/temper-app", version = "2.0.0-alpha.1" }
serde = { version = "1.0.200", default-features = false }

[profile.release]
version = "not a site"
"#;

    const LOCK: &str = r#"version = 4

[[package]]
name = "serde"
version = "1.0.200"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "temper-app"
version = "2.0.0-alpha.0"
dependencies = [
 "temper-domain",
]

[[package]]
name = "xtask"
version = "2.0.0-alpha.0"
"#;

    fn names(sites: &[Site]) -> Vec<(&str, &str)> {
        sites
            .iter()
            .map(|s| (s.name.as_str(), s.version.as_str()))
            .collect()
    }

    #[test]
    fn cargo_sites_are_the_workspace_and_sibling_versions() {
        assert_eq!(
            names(&cargo_sites(MANIFEST)),
            [
                ("Cargo.toml [workspace.package]", "2.0.0-alpha.0"),
                (
                    "Cargo.toml [workspace.dependencies] temper-domain",
                    "2.0.0-alpha.0"
                ),
                (
                    "Cargo.toml [workspace.dependencies] temper-app",
                    "2.0.0-alpha.1"
                ),
            ],
            "sites in file order; serde and [profile] are not sites"
        );
    }

    #[test]
    fn set_cargo_versions_rewrites_only_the_sites() {
        let set = set_cargo_versions(MANIFEST, "2.0.0");
        let expected = MANIFEST
            .replace(
                "version = \"2.0.0-alpha.0\"      # comment",
                "version = \"2.0.0\"      # comment",
            )
            .replace(
                "temper-domain\", version = \"2.0.0-alpha.0\"",
                "temper-domain\", version = \"2.0.0\"",
            )
            .replace(
                "temper-app\", version = \"2.0.0-alpha.1\"",
                "temper-app\", version = \"2.0.0\"",
            );
        assert_eq!(set, expected, "rewritten manifest");
    }

    #[test]
    fn lock_sites_are_the_members_without_a_source() {
        assert_eq!(
            names(&lock_sites(LOCK)),
            [
                ("Cargo.lock temper-app", "2.0.0-alpha.0"),
                ("Cargo.lock xtask", "2.0.0-alpha.0")
            ],
            "registry packages are not sites"
        );
        let set = set_lock_versions(LOCK, "2.0.0");
        assert_eq!(
            names(&lock_sites(&set)),
            [
                ("Cargo.lock temper-app", "2.0.0"),
                ("Cargo.lock xtask", "2.0.0")
            ],
            "members rewritten"
        );
        assert!(set.contains("version = \"1.0.200\""), "serde untouched");
    }

    #[test]
    fn package_json_sites_are_the_version_and_the_pins() -> Result<(), serde_json::Error> {
        let text = r#"{
  "name": "temper-plugin",
  "version": "2.0.0",
  "optionalDependencies": {
    "@wet-ink/temper-linux-x64": "2.0.0",
    "@wet-ink/temper-win32-arm64": "2.0.1"
  }
}"#;
        assert_eq!(
            names(&package_json_sites(text)?),
            [
                ("package.json version", "2.0.0"),
                ("package.json @wet-ink/temper-linux-x64", "2.0.0"),
                ("package.json @wet-ink/temper-linux-arm64", ""),
                ("package.json @wet-ink/temper-darwin-x64", ""),
                ("package.json @wet-ink/temper-darwin-arm64", ""),
                ("package.json @wet-ink/temper-win32-x64", ""),
                ("package.json @wet-ink/temper-win32-arm64", "2.0.1"),
            ],
            "the version and every engine's pin, absent ones empty"
        );
        let set = set_json_strings(
            text,
            &[
                "version",
                "@wet-ink/temper-linux-x64",
                "@wet-ink/temper-win32-arm64",
            ],
            "2.1.0",
        );
        assert_eq!(
            set,
            text.replace("2.0.0", "2.1.0").replace("2.0.1", "2.1.0"),
            "formatting kept"
        );
        Ok(())
    }

    #[test]
    fn mismatches_name_each_disagreeing_site() {
        let sites = [
            Site::new("a", "2.0.0"),
            Site::new("b", "2.0.1"),
            Site::new("c", "2.0.0"),
        ];
        assert_eq!(
            mismatches(&sites, None)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["b is 2.0.1, expected 2.0.0"],
            "against the first site"
        );
        assert_eq!(
            mismatches(&sites, Some("2.0.1")).len(),
            2,
            "against an expected version"
        );
        assert!(mismatches(&[], None).is_empty(), "no sites, no mismatch");
    }

    #[test]
    fn json_version_reads_the_top_level_version() -> Result<(), serde_json::Error> {
        assert_eq!(
            json_version(r#"{"name":"temper","version":"1.5.0"}"#)?.as_deref(),
            Some("1.5.0"),
            "plugin.json"
        );
        assert!(json_version("{").is_err(), "invalid JSON");
        Ok(())
    }

    #[test]
    fn changelog_heading_and_prerelease() {
        let changelog = "# Changelog\n\n## Unreleased\n\n## 2.0.0 (2026-11-01)\n";
        assert!(has_changelog_heading(changelog, "2.0.0"), "dated heading");
        assert!(!has_changelog_heading(changelog, "2.0"), "prefix only");
        assert!(is_prerelease("2.0.0-alpha.1"), "alpha");
        assert!(!is_prerelease("2.0.0"), "release");
    }
}
