//! Cargo manifests as 1.x reads them, and the manifest policy the pre-edit hook and the gate
//! apply (1.x `manifest.mjs` and the manifest helpers of `hooks.mjs`).

use std::str::FromStr;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::js::{Object, Value, integer_from_radix, number_from_text};

mod checks;
mod deps;
mod problems;
mod requirement;
mod sections;
mod watched;

pub use checks::{Before, ManifestEditFacts, ManifestFindings, manifest_checks};
pub use deps::{DepKind, DepName, dep_names, dep_requirements, dependency_names};
#[cfg(test)]
pub(crate) use problems::MOCKING_CRATES;
pub use problems::{
    GoverningRoot, ManifestProblem, OwnLints, RootRelation, SplitProblems, member_problems,
    split_problems,
};
pub use requirement::{major_of, non_caret_requirement};
pub use sections::{lint_config_changed, lint_sections};
pub use watched::{WatchedKey, benign_when_new, watched_keys};

/// 1.x `inheritsLints`: `m?.lints?.workspace === true`.
fn inherits_lints(m: &Manifest) -> bool {
    m.get("lints").and_then(|l| l.get("workspace")) == Some(&Value::Bool(true))
}

/// A parsed `Cargo.toml`, held in the JS data model 1.x's `toml.mjs` builds: numbers are JS
/// numbers, a date or time is its source text, and tables keep JS property order.
///
/// Parsing goes through the `toml` crate (port plan D16(c)), so a manifest that `toml` rejects
/// (duplicate keys, malformed values) is invalid here even where `toml.mjs` was lenient.
///
/// # Examples
///
/// ```
/// use temper_domain::Manifest;
///
/// assert!("[package]\nname = \"a\"\n".parse::<Manifest>().is_ok());
/// assert!("[package\n".parse::<Manifest>().is_err());
/// ```
#[derive(Debug)]
pub struct Manifest(Object);

/// The text is not valid TOML.
#[derive(Debug, thiserror::Error)]
#[error("the manifest is not valid TOML")]
pub struct ManifestParseError(#[source] toml::de::Error);

impl FromStr for Manifest {
    type Err = ManifestParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let table = DeTable::parse(text).map_err(ManifestParseError)?;
        Ok(Self(object(text, table.get_ref())))
    }
}

impl Manifest {
    /// JS `{}`: what 1.x compares a brand-new manifest's `before` side as.
    pub(crate) fn empty() -> Self {
        Self(Object::default())
    }

    /// The top-level key `key` (JS `m?.[key]`).
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

/// One TOML table as `toml.mjs` returns it. Its `obj[k] = v` never makes an own `__proto__`
/// key: a table value becomes the prototype instead, which the port does not emulate (its keys
/// are not inherited here), and any other value is ignored. Either way 1.x never sees the key,
/// so it is dropped.
fn object(source: &str, table: &DeTable<'_>) -> Object {
    table
        .iter()
        .filter(|(k, _)| k.get_ref() != "__proto__")
        .map(|(k, v)| (k.get_ref().to_string(), value(source, v)))
        .collect()
}

/// One TOML value as `toml.mjs` returns it.
fn value(source: &str, spanned: &Spanned<DeValue<'_>>) -> Value {
    match spanned.get_ref() {
        DeValue::String(s) => Value::String(s.to_string()),
        DeValue::Integer(i) => Value::Number(integer_from_radix(i.as_str(), i.radix())),
        DeValue::Float(f) => Value::Number(number_from_text(f.as_str())),
        DeValue::Boolean(b) => Value::Bool(*b),
        // `toml.mjs` keeps the source text of a date or time.
        DeValue::Datetime(d) => Value::String(
            source
                .get(spanned.span())
                .map_or_else(|| d.to_string(), str::to_owned),
        ),
        DeValue::Array(items) => Value::Array(items.iter().map(|v| value(source, v)).collect()),
        DeValue::Table(t) => Value::Object(object(source, t)),
    }
}

#[cfg(test)]
mod tests;
