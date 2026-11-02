//! Manifest switches whose change is a human decision (1.x `watchedKeys`): H-12 switches that
//! can stop tests, doctests or targets from running, and H-16 build, source-override, MSRV,
//! edition, resolver and default-feature settings.

use crate::js::{Value, js_json, truthy};

use super::Manifest;

/// One watched switch: its rule, why it is watched, and its value as `JSON.stringify` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedKey {
    /// `H-12` or `H-16`.
    pub id: &'static str,
    /// Why a change is a human decision.
    pub why: &'static str,
    /// The value, as `JSON.stringify` prints it.
    pub value: String,
}

const WHY_TARGET: &str = "this switch can stop tests, doctests or targets from running (H-12)";
const WHY_AUTO: &str =
    "turning off target discovery means those targets are never built or tested (H-12)";
const WHY_AUTOBINS: &str =
    "turning off binary discovery means binaries under src/bin are never built or checked (H-12)";
const WHY_BUILD: &str = "it changes or disables the build script (ARC-17, H-16)";
const WHY_MSRV: &str = "MSRV, edition and resolver changes are human decisions (H-16)";
const WHY_FEATURES: &str =
    "default features change what every user of the crate gets (API-16, H-16)";
const WHY_PATCH: &str = "it overrides where a dependency comes from (H-16)";

const TARGET_KEYS: [&str; 5] = ["test", "doctest", "harness", "bench", "required-features"];

/// The watched switches a manifest sets, as `(label, key)` in 1.x's Map insertion order.
///
/// # Examples
///
/// ```
/// use temper_domain::{Manifest, watched_keys};
///
/// let m: Manifest = "[lib]\ndoctest = false\n".parse()?;
/// let watched = watched_keys(&m);
/// assert_eq!(watched.iter().map(|(label, w)| (label.as_str(), w.id)).collect::<Vec<_>>(), [("[lib] doctest", "H-12")]);
/// # Ok::<(), temper_domain::ManifestParseError>(())
/// ```
#[must_use]
pub fn watched_keys(m: &Manifest) -> Vec<(String, WatchedKey)> {
    let mut out = Watched(Vec::new());
    out.package_keys(m);
    out.target_keys(m);
    out.0
}

/// A rule id and why its switches are watched.
type Why = (&'static str, &'static str);

/// 1.x's `out` Map, in insertion order.
struct Watched(Vec<(String, WatchedKey)>);

impl Watched {
    /// 1.x's `put`: a value that is set is recorded; JS `map.set` keeps a repeated label's place
    /// and takes its new value.
    fn put(&mut self, (id, why): Why, label: String, v: Option<&Value>) {
        let Some(v) = v else { return };
        let entry = WatchedKey {
            id,
            why,
            value: js_json(v),
        };
        match self.0.iter_mut().find(|(l, _)| *l == label) {
            Some(slot) => slot.1 = entry,
            None => self.0.push((label, entry)),
        }
    }

    fn package_keys(&mut self, m: &Manifest) {
        let pkg = m.get("package");
        let pkg_key = |k: &str| pkg.and_then(|p| p.get(k));
        for k in ["autotests", "autobenches", "autoexamples", "autolib"] {
            self.put(("H-12", WHY_AUTO), format!("package.{k}"), pkg_key(k));
        }
        self.put(
            ("H-12", WHY_AUTOBINS),
            "package.autobins".to_owned(),
            pkg_key("autobins"),
        );
        self.put(
            ("H-16", WHY_BUILD),
            "package.build".to_owned(),
            pkg_key("build"),
        );
        let workspace = m.get("workspace");
        for k in ["rust-version", "edition", "resolver"] {
            self.put(("H-16", WHY_MSRV), format!("package.{k}"), pkg_key(k));
            let ws_pkg = workspace
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get(k));
            self.put(("H-16", WHY_MSRV), format!("workspace.package.{k}"), ws_pkg);
        }
        let resolver = workspace.and_then(|w| w.get("resolver"));
        self.put(
            ("H-16", WHY_MSRV),
            "workspace.resolver".to_owned(),
            resolver,
        );
        let default_features = m.get("features").and_then(|f| f.get("default"));
        self.put(
            ("H-16", WHY_FEATURES),
            "features.default".to_owned(),
            default_features,
        );
        self.put(("H-16", WHY_PATCH), "patch".to_owned(), m.get("patch"));
        self.put(("H-16", WHY_PATCH), "replace".to_owned(), m.get("replace"));
    }

    fn target_keys(&mut self, m: &Manifest) {
        for (kind, list) in targets(m) {
            for (i, t) in list.iter().enumerate() {
                let head = match (kind, t.get("name").and_then(Value::as_str)) {
                    ("lib", _) => "[lib]".to_owned(),
                    (_, Some(name)) => format!("[[{kind}]] {name}"),
                    (_, None) => format!("[[{kind}]] #{}", i.saturating_add(1)),
                };
                for k in TARGET_KEYS {
                    // criterion and divan need `harness = false`; `bench = false` on a lib or bin
                    // keeps libtest's bench harness out of criterion's way.
                    let skipped = (kind == "bench" && k == "harness")
                        || (matches!(kind, "lib" | "bin") && k == "bench");
                    if !skipped {
                        self.put(("H-12", WHY_TARGET), format!("{head} {k}"), t.get(k));
                    }
                }
            }
        }
    }
}

/// 1.x's target lists: `[lib]` when truthy, then each `[[bin]]`, `[[test]]`, `[[bench]]` and
/// `[[example]]` array (anything else reads as empty).
fn targets(m: &Manifest) -> [(&'static str, &[Value]); 5] {
    let lib = m
        .get("lib")
        .filter(|lib| truthy(lib))
        .map_or(&[][..], std::slice::from_ref);
    let list = |kind: &str| match m.get(kind) {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    };
    [
        ("lib", lib),
        ("bin", list("bin")),
        ("test", list("test")),
        ("bench", list("bench")),
        ("example", list("example")),
    ]
}

/// Is this watched value benign in a brand-new manifest (1.x `benignWhenNew`)?
///
/// Plain inheritance from the workspace, the standard's edition (2024) and resolver (3), and
/// the crate's own default features are.
#[must_use]
pub fn benign_when_new(label: &str, entry: &WatchedKey) -> bool {
    if entry.value == r#"{"workspace":true}"# {
        return true;
    }
    let last = |name: &str| label == name || label.ends_with(&format!(".{name}"));
    if last("edition") {
        return entry.value == r#""2024""#;
    }
    if last("resolver") {
        return entry.value == r#""3""#;
    }
    label == "features.default"
}
