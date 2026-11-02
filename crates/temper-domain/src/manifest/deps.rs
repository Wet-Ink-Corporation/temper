//! Declared dependencies: their names per table and their version requirements.

use std::fmt;

use crate::js::{JsMap, Value, entries, is_js_space, js_trim};

use super::Manifest;
use super::sections::{header, lines};

/// Which dependency table a dependency is declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepKind {
    /// `[dependencies]`, and `[workspace.dependencies]`.
    Normal,
    /// `[dev-dependencies]`.
    Dev,
    /// `[build-dependencies]`.
    Build,
}

impl DepKind {
    const ALL: [Self; 3] = [Self::Normal, Self::Dev, Self::Build];

    /// The table name: `dependencies`, `dev-dependencies` or `build-dependencies`.
    #[must_use]
    pub const fn table(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Dev => "dev-dependencies",
            Self::Build => "build-dependencies",
        }
    }

    fn of_table(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.table() == name)
    }
}

/// A declared dependency: its table kind and its key. `Display` is 1.x's `kind:name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DepName {
    /// The table it is declared in.
    pub kind: DepKind,
    /// The dependency key (not a `package =` rename).
    pub name: String,
}

impl fmt::Display for DepName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.table(), self.name)
    }
}

/// 1.x's `names.add(...)` of a `kind:name` key on a JS `Set`: a repeat keeps its first place.
fn add(names: &mut JsMap<DepName, ()>, kind: DepKind, name: &str) {
    names.add(DepName {
        kind,
        name: name.to_owned(),
    });
}

/// Every declared dependency, each once, in 1.x's `dependencyNames` order: the three tables,
/// then `[workspace.dependencies]` (as normal ones), then each `[target.*]` table's three.
///
/// # Examples
///
/// ```
/// use temper_domain::{Manifest, dependency_names};
///
/// let m: Manifest = "[dev-dependencies]\nproptest = \"1\"\n".parse()?;
/// let names: Vec<_> = dependency_names(&m).iter().map(ToString::to_string).collect();
/// assert_eq!(names, ["dev-dependencies:proptest"]);
/// # Ok::<(), temper_domain::ManifestParseError>(())
/// ```
#[must_use]
pub fn dependency_names(m: &Manifest) -> Vec<DepName> {
    let mut names = JsMap::new();
    let mut visit = |kind, deps: Option<&Value>| {
        for (name, _) in deps.into_iter().flat_map(entries) {
            add(&mut names, kind, &name);
        }
    };
    for kind in DepKind::ALL {
        visit(kind, m.get(kind.table()));
    }
    visit(
        DepKind::Normal,
        m.get("workspace").and_then(|w| w.get("dependencies")),
    );
    for (_, target) in m.get("target").into_iter().flat_map(entries) {
        for kind in DepKind::ALL {
            visit(kind, target.get(kind.table()));
        }
    }
    names.into_keys()
}

/// The dependency names in a manifest's text, as 1.x's `depNames`: from the parse, or, when the
/// text is not valid TOML, from its header and key lines.
#[must_use]
pub fn dep_names(text: &str) -> Vec<DepName> {
    match text.parse::<Manifest>() {
        Ok(m) => dependency_names(&m),
        // Invalid TOML: 1.x falls back to reading lines, as cargo would reject the file anyway.
        Err(_) => dep_names_by_line(text),
    }
}

/// 1.x's `depNamesByLine`: `[..dependencies.name]` headers, and `name =` lines under a
/// `[..dependencies]` header.
fn dep_names_by_line(text: &str) -> Vec<DepName> {
    let mut names = JsMap::new();
    let mut table = None;
    for line in lines(text) {
        if let Some(name) = header(line) {
            let name = js_trim(name);
            table = None;
            match inline_dependency(name) {
                Some((kind, dep)) => add(&mut names, kind, dep),
                None => table = name.rsplit('.').next().and_then(DepKind::of_table),
            }
            continue;
        }
        if let Some(kind) = table
            && let Some(key) = key_of(line)
        {
            add(&mut names, kind, key);
        }
    }
    names.into_keys()
}

/// 1.x `/(?:^|\.)((?:dev-|build-)?dependencies)\.([\w-]+)$/`: a header naming one dependency.
fn inline_dependency(name: &str) -> Option<(DepKind, &str)> {
    let (table, dep) = name.rsplit_once('.')?;
    let kind = DepKind::of_table(table.rsplit('.').next()?)?;
    (!dep.is_empty() && dep.chars().all(is_key_char)).then_some((kind, dep))
}

/// 1.x `/^\s*([A-Za-z0-9_-]+)\s*=/`: the bare key of a `key = value` line.
fn key_of(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(is_js_space);
    let end = rest.find(|c| !is_key_char(c)).unwrap_or(rest.len());
    let (key, after) = rest.split_at_checked(end)?;
    let assigns = after.trim_start_matches(is_js_space).starts_with('=');
    (!key.is_empty() && assigns).then_some(key)
}

const fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Dependency version requirements as `table:name` and the requirement.
///
/// The order is 1.x's `depRequirements`: the three tables, `workspace.dependencies`, then
/// `target.<cfg>.<table>`. A dependency without a string requirement (a path, git or
/// `workspace = true` one) is left out.
///
/// # Examples
///
/// ```
/// use temper_domain::{Manifest, dep_requirements};
///
/// let m: Manifest = "[dependencies]\nserde = { version = \"1\" }\n".parse()?;
/// assert_eq!(dep_requirements(&m), [("dependencies:serde".to_owned(), "1".to_owned())]);
/// # Ok::<(), temper_domain::ManifestParseError>(())
/// ```
#[must_use]
pub fn dep_requirements(m: &Manifest) -> Vec<(String, String)> {
    // 1.x's `out` Map: a key seen twice keeps its first place and takes its last requirement.
    let mut out = JsMap::new();
    let mut visit = |label: &str, deps: Option<&Value>| {
        for (name, spec) in deps.into_iter().flat_map(entries) {
            let req = spec
                .as_str()
                .or_else(|| spec.get("version").and_then(Value::as_str));
            if let Some(req) = req {
                out.set(format!("{label}:{name}"), req.to_owned());
            }
        }
    };
    for kind in DepKind::ALL {
        visit(kind.table(), m.get(kind.table()));
    }
    visit(
        "workspace.dependencies",
        m.get("workspace").and_then(|w| w.get("dependencies")),
    );
    for (cfg, target) in m.get("target").into_iter().flat_map(entries) {
        for kind in DepKind::ALL {
            visit(
                &format!("target.{cfg}.{}", kind.table()),
                target.get(kind.table()),
            );
        }
    }
    out.into_vec()
}
