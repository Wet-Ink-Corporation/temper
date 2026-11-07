//! `temper status --json` end to end, with real git (port Phase 3c): the change-detection
//! tests of 1.x's `handlers.test.mjs` through the document's `changed` and `fingerprint`, the
//! `--session` base, and the pid probe on this OS.

use std::fs;
use std::path::Path;
use std::process::Command;

use temper_domain::JsValue;

use crate::support::{TestResult, git, git_init};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const MANIFEST: &str = "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";

/// A one-crate tree at a temporary root, committed when `git` is set.
struct Tree(tempfile::TempDir);

impl Tree {
    fn new(with_git: bool) -> Res<Self> {
        let tree = Self(tempfile::tempdir()?);
        tree.put("Cargo.toml", MANIFEST)?;
        tree.put("src/lib.rs", "pub fn f() {}\n")?;
        if with_git {
            git_init(tree.path())?;
            tree.commit("init")?;
        }
        Ok(tree)
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn put(&self, rel: &str, text: &str) -> TestResult {
        let file = self.path().join(rel);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, text)?;
        Ok(())
    }

    fn commit(&self, message: &str) -> TestResult {
        git(self.path(), &["add", "-A"])?;
        git(self.path(), &["commit", "-qm", message])
    }

    /// `temper status --json <args>` here, without the caller's `TEMPER_BASE` or
    /// `TEMPER_DISABLE`: the document.
    fn doc(&self, args: &[&str]) -> Res<JsValue> {
        let output = Command::new(env!("CARGO_BIN_EXE_temper"))
            .args(["status", "--json"])
            .args(args)
            .current_dir(self.path())
            .env("GIT_CEILING_DIRECTORIES", std::env::temp_dir())
            .env_remove("TEMPER_BASE")
            .env_remove("TEMPER_DISABLE")
            .output()?;
        if output.status.code() != Some(0) {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(JsValue::from_json(&String::from_utf8(output.stdout)?)?)
    }

    fn fp(&self) -> Res<String> {
        let doc = self.doc(&[])?;
        Ok(doc
            .get("fingerprint")
            .and_then(JsValue::as_str)
            .ok_or("a fingerprint")?
            .to_owned())
    }

    /// The document's `changed`, sorted.
    fn changed(&self) -> Res<Vec<String>> {
        changed_of(&self.doc(&[])?)
    }
}

fn changed_of(doc: &JsValue) -> Res<Vec<String>> {
    let Some(JsValue::Array(items)) = doc.get("changed") else {
        return Err("a changed list".into());
    };
    let mut changed = items
        .iter()
        .map(|item| item.as_str().map(str::to_owned).ok_or("a path"))
        .collect::<Result<Vec<_>, _>>()?;
    changed.sort();
    Ok(changed)
}

/// Ported "change detection: .claude/temper.json and .cargo/config(.toml) count as Rust
/// changes and change the fingerprint" (its `changed` and fingerprint assertions).
#[test]
fn temper_json_and_cargo_config_count_as_rust_changes_and_change_the_fingerprint() -> TestResult {
    let r = Tree::new(true)?;
    r.put(".claude/temper.json", r#"{"review":"required"}"#)?;
    r.put(".cargo/config.toml", "[build]\nrustflags = []\n")?;
    r.commit("config")?;
    let clean = r.fp()?;
    assert_eq!(r.changed()?, Vec::<String>::new());
    r.put(
        ".claude/temper.json",
        r#"{"review":"required","gate":{"allFeatures":false}}"#,
    )?;
    assert_eq!(r.changed()?, [".claude/temper.json"]);
    let after_json = r.fp()?;
    assert_ne!(
        after_json, clean,
        "an edited .claude/temper.json changes the fingerprint"
    );
    r.put(
        ".cargo/config.toml",
        "[build]\nrustflags = [\"-Dwarnings\"]\n",
    )?;
    assert_eq!(r.changed()?, [".cargo/config.toml", ".claude/temper.json"]);
    assert_ne!(
        r.fp()?,
        after_json,
        "an edited .cargo/config.toml changes the fingerprint"
    );

    let u = Tree::new(true)?;
    let mut fp = u.fp()?;
    for rel in [
        ".claude/temper.json",
        ".cargo/config",
        "crates/core/.cargo/config.toml",
    ] {
        let text = if rel == ".claude/temper.json" {
            r#"{"gate":{"doc":false}}"#
        } else {
            "[build]\nrustflags = []\n"
        };
        u.put(rel, text)?;
        assert!(
            u.changed()?.iter().any(|c| c == rel),
            "{rel} is a Rust change"
        );
        let next = u.fp()?;
        assert_ne!(next, fp, "untracked {rel} changes the fingerprint");
        fp = next;
    }
    Ok(())
}

/// Ported "change detection: other files under .claude/ and .cargo/ are not Rust changes and
/// leave the fingerprint alone" (without its `stop` assertion: Phase 4).
#[test]
fn other_files_under_claude_and_cargo_are_no_rust_changes() -> TestResult {
    let r = Tree::new(true)?;
    let fp = r.fp()?;
    for rel in [
        ".claude/settings.json",
        ".claude/settings.local.json",
        ".claude/temper.json.bak",
        ".claude/temper-notes.json",
        "docs/.claude/temper.json",
        ".cargo/credentials.toml",
        ".cargo/config.toml.orig",
        "notes/cargo-config.toml",
        "NOTES.md",
    ] {
        r.put(rel, "{}\n")?;
    }

    assert_eq!((r.changed()?, r.fp()?), (Vec::new(), fp));
    Ok(())
}

/// Ported "change detection: .claude/temper.json counts only when a key the gate reads (gate,
/// scanner, manifests) changes" (without its `stop` assertions: Phase 4).
#[test]
fn temper_json_counts_only_when_a_key_the_gate_reads_changes() -> TestResult {
    let r = Tree::new(true)?;
    r.put(
        ".claude/temper.json",
        r#"{"review":"required","gate":{"allFeatures":true}}"#,
    )?;
    r.commit("config")?;
    let fp = r.fp()?;
    for cfg in [
        "{\n  \"review\": \"off\",\n  \"gate\": {\n    \"allFeatures\": true\n  }\n}\n",
        r#"{"review":"advisory","skillGate":false,"askOnNewDependency":false,"preapprovedDependencies":["rand"],"protected":[],"gate":{"allFeatures":true}}"#,
        r#"{"gate":{"doc":true,"allFeatures":true},"review":"required","scanner":{"exempt":[]}}"#,
    ] {
        r.put(".claude/temper.json", cfg)?;
        assert_eq!((r.changed()?, r.fp()?), (Vec::new(), fp.clone()), "{cfg}");
    }
    for cfg in [
        r#"{"review":"required","gate":{"allFeatures":false}}"#,
        r#"{"review":"required","gate":{"allFeatures":true},"scanner":{"exempt":[{"rule":"H-01","paths":["**"]}]}}"#,
        r#"{"review":"required","gate":{"allFeatures":true},"manifests":{"ownLints":["crates/core"]}}"#,
    ] {
        r.put(".claude/temper.json", cfg)?;
        assert_eq!(r.changed()?, [".claude/temper.json"], "{cfg}");
        assert_ne!(r.fp()?, fp, "{cfg}");
    }
    fs::remove_file(r.path().join(".claude/temper.json"))?;
    assert_eq!(
        (r.changed()?, r.fp()?),
        (Vec::new(), fp),
        "a deleted file is the defaults"
    );
    Ok(())
}

/// Ported "change detection: a property the gate does not read under gate, scanner or
/// manifests is no Rust change".
#[test]
fn a_property_the_gate_does_not_read_is_no_rust_change() -> TestResult {
    let r = Tree::new(true)?;
    r.put(".claude/temper.json", r#"{"gate":{"allFeatures":true}}"#)?;
    r.commit("config")?;
    let fp = r.fp()?;
    for cfg in [
        r#"{"gate":{"allFeatures":true,"colour":"blue"}}"#,
        r#"{"gate":{"allFeatures":true},"scanner":{"exempt":[],"note":"see ADR-7"}}"#,
        r#"{"gate":{"allFeatures":true},"manifests":{"ownLints":[],"owner":"ffi team"}}"#,
    ] {
        r.put(".claude/temper.json", cfg)?;
        assert_eq!((r.changed()?, r.fp()?), (Vec::new(), fp.clone()), "{cfg}");
    }
    Ok(())
}

/// Ported "change detection outside git: config files count, and the fingerprint is stable
/// and follows only the gate settings" (a one-crate tree: every Rust file is a change).
#[test]
fn outside_git_config_files_count_and_the_fingerprint_follows_the_gate_settings() -> TestResult {
    let t = Tree::new(false)?;
    let start = t.fp()?;
    assert_eq!(t.fp()?, start, "the same tree hashes the same across calls");
    assert_eq!(t.changed()?, ["Cargo.toml", "src/lib.rs"]);
    for cfg in [
        r#"{"review":"off","skillGate":false}"#,
        r#"{"gate":{"doc":true},"scanner":{"exempt":[]}}"#,
        r#"{"gate":{"colour":"blue"},"manifests":{"owner":"x"}}"#,
    ] {
        t.put(".claude/temper.json", cfg)?;
        assert_eq!(t.fp()?, start, "{cfg}");
        assert!(
            !t.changed()?.iter().any(|c| c == ".claude/temper.json"),
            "{cfg}"
        );
    }
    t.put(".claude/temper.json", r#"{"gate":{"allFeatures":false}}"#)?;
    let after_json = t.fp()?;
    assert_ne!(after_json, start, "a gate setting changes the fingerprint");
    assert_eq!(t.fp()?, after_json, "and stays stable");
    assert!(t.changed()?.iter().any(|c| c == ".claude/temper.json"));
    t.put(".cargo/config.toml", "[build]\nrustflags = []\n")?;
    let after_cargo = t.fp()?;
    assert_ne!(
        after_cargo, after_json,
        "a .cargo/config.toml changes the fingerprint"
    );
    assert!(t.changed()?.iter().any(|c| c == ".cargo/config.toml"));
    t.put(
        ".cargo/config.toml",
        "[build]\nrustflags = [\"-Dwarnings\"]\n",
    )?;
    assert_ne!(t.fp()?, after_cargo, "so does an edit to it");
    let settled = t.fp()?;
    t.put(".claude/settings.json", "{}\n")?;
    t.put(".cargo/credentials.toml", "{}\n")?;
    assert_eq!(t.fp()?, settled);
    assert!(
        !t.changed()?
            .iter()
            .any(|c| c.ends_with("settings.json") || c.ends_with("credentials.toml"))
    );
    Ok(())
}

/// Ported "change detection: a git-ignored .claude/temper.json still counts, as temper reads
/// it".
#[test]
fn a_git_ignored_temper_json_still_counts() -> TestResult {
    let r = Tree::new(true)?;
    r.put(".gitignore", ".claude/\n")?;
    r.commit("ignore")?;
    let fp = r.fp()?;
    r.put(".claude/temper.json", r#"{"review":"off"}"#)?;
    assert_eq!((r.changed()?, r.fp()?), (Vec::new(), fp.clone()));
    r.put(".claude/temper.json", r#"{"gate":{"allFeatures":false}}"#)?;
    assert_eq!(r.changed()?, [".claude/temper.json"]);
    assert_ne!(r.fp()?, fp);
    Ok(())
}

/// Ported "status --json --session: the session's recorded base wins over HEAD", with real
/// git: the session's base, changes and fingerprint; an unknown session falls back to HEAD.
#[test]
fn a_sessions_recorded_base_wins_over_head() -> TestResult {
    let r = Tree::new(true)?;
    r.put("src/lib.rs", "pub fn f() {}\npub fn g() {}\n")?;
    let first = r.doc(&[])?;
    r.commit("b")?;
    let base_of = |doc: &JsValue| doc.get("base").and_then(JsValue::as_str).map(str::to_owned);
    let first_base = base_of(&first).ok_or("a base")?;
    r.put(".git/temper/sessions/s1/base", &first_base)?;

    let plain = r.doc(&[])?;
    let session = r.doc(&["--session", "s1"])?;
    let nobody = r.doc(&["--session", "nobody"])?;

    assert_ne!(base_of(&plain), Some(first_base.clone()), "HEAD moved");
    assert_eq!(changed_of(&plain)?, Vec::<String>::new());
    assert_eq!(base_of(&session), Some(first_base));
    assert_eq!(changed_of(&session)?, ["src/lib.rs"]);
    assert_ne!(
        session.get("fingerprint"),
        plain.get("fingerprint"),
        "hashed against its base"
    );
    assert_eq!(base_of(&nobody), base_of(&plain));
    Ok(())
}

/// OC-2 A on this OS (`/proc` on Linux, `ps` on macOS, `tasklist` on Windows): a progress file
/// naming this test's own process shows as a running gate; one naming 2147483646, which no
/// process has, does not. The start is in the far future, so no age makes it stale.
#[test]
fn a_running_gate_shows_for_a_live_pid_and_not_for_a_dead_one() -> TestResult {
    let r = Tree::new(true)?;
    let fp = r.fp()?;
    let progress = |pid: u32| {
        format!(
            r#"{{"fp":"{fp}","pid":{pid},"startedAt":"+275760-09-13T00:00:00.000Z","steps":[],"current":null}}"#
        )
    };
    let file = format!(".git/temper/gate-progress-{fp}.json");
    let running = |doc: &JsValue| {
        doc.get("gate")
            .and_then(|g| g.get("running"))
            .map(JsValue::to_json)
    };

    r.put(&file, &progress(std::process::id()))?;
    let live = r.doc(&[])?;
    r.put(&file, &progress(2_147_483_646))?;
    let dead = r.doc(&[])?;

    assert_eq!(
        running(&live).as_deref(),
        Some(r#"{"startedAt":"+275760-09-13T00:00:00.000Z","steps":[],"current":null}"#)
    );
    assert_eq!(
        dead.get("gate"),
        Some(&JsValue::Null),
        "no gate and none running"
    );
    Ok(())
}
