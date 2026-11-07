//! `temper status --json`'s document (1.x `scripts/temper.mjs` `statusJson`).
//!
//! The document is written field by field, in 1.x's key order, with JS's `JSON.stringify`
//! for every value copied from a state file, so nothing read from a file is cloned.

use super::{BaselineLine, StatusFacts, review_name};
use crate::js::{Object, Value, js_json, json_string, truthy};
use crate::{BASELINE_FILE, GateRecord};

/// What `status --json` adds to the facts the text prints.
#[derive(Debug, Clone, Copy)]
pub struct StatusDoc<'a> {
    /// temper's version (`TEMPER_PLUGIN_VERSION`, else the engine's own, port plan 3-13).
    pub version: &'a str,
    /// The progress of a gate running now for the fingerprint ([`crate::gate_progress`]).
    pub running: Option<&'a Value>,
    /// `TEMPER_DISABLE=1`: the document says `"disabled": true`.
    pub disabled: bool,
}

/// `temper status --json`'s document, `\n`-terminated, as 1.x's `JSON.stringify(doc)` writes it.
///
/// The keys are `temper_status: 1`, then the version, root, state dir, base, fingerprint,
/// changes, the gate (or the running gate's progress), the review, the baseline and the config.
///
/// A gate or verdict record that is not a plain object reads as none, and a step or finding
/// that is not one is dropped; a key whose 1.x value is `undefined` is left out. A config
/// problem adds `config.problem` after `skillGate`, and only then (OC-5 A, wi-018023).
///
/// # Examples
///
/// ```
/// use temper_domain::{BaselineLine, Config, StatusDoc, StatusFacts, render_status_json};
///
/// let config = Config::defaults();
/// let facts = StatusFacts {
///     root: "/w", state_dir: "/w/.git/temper", base: None,
///     fingerprint: "0123456789abcdef".parse()?, changed: &[], gate: None, review: None,
///     baseline: BaselineLine::Absent, config: &config, config_problem: None,
/// };
/// let doc = StatusDoc { version: "1.6.0", running: None, disabled: false };
/// assert!(render_status_json(&facts, &doc).starts_with(r#"{"temper_status":1,"version":"1.6.0","#));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn render_status_json(facts: &StatusFacts<'_>, doc: &StatusDoc<'_>) -> String {
    let fp = facts.fingerprint.to_string();
    let mut out = JsonObject::new();
    out.raw("temper_status", "1");
    out.raw("version", &json_string(doc.version));
    out.raw("root", &json_string(facts.root));
    out.raw("stateDir", &json_string(facts.state_dir));
    out.raw("base", &facts.base.map_or_else(null, json_string));
    out.raw("fingerprint", &json_string(&fp));
    out.raw("changed", &string_array(facts.changed));
    out.raw("gate", &gate_json(facts.gate, doc.running));
    out.raw("review", &review_json(facts.review, &fp));
    out.raw("baseline", &baseline_json(&facts.baseline));
    let mut config = JsonObject::new();
    config.raw("review", &json_string(review_name(facts.config.review())));
    config.raw("skillGate", bool_json(facts.config.skill_gate()));
    if let Some(problem) = facts.config_problem {
        config.raw("problem", &json_string(&problem.to_string()));
    }
    out.raw("config", &config.finish());
    if doc.disabled {
        out.raw("disabled", "true");
    }
    let mut json = out.finish();
    json.push('\n');
    json
}

/// `status --json` outside a Rust project: `{ temper_status: 1, version, project: null }`,
/// `\n`-terminated.
#[must_use]
pub fn render_no_project_json(version: &str) -> String {
    let mut out = JsonObject::new();
    out.raw("temper_status", "1");
    out.raw("version", &json_string(version));
    out.raw("project", "null");
    let mut json = out.finish();
    json.push('\n');
    json
}

/// The `gate` field: `null` without a record or a running gate, else 1.x's object.
fn gate_json(gate: Option<&GateRecord>, running: Option<&Value>) -> String {
    let g = gate.and_then(|g| record(Some(g.value())));
    if g.is_none() && running.is_none() {
        return null();
    }
    let field = |key: &str| g.and_then(|g| g.get(key));
    let mut out = JsonObject::new();
    match g {
        // `g ? g.ok : null`: a record without `ok` leaves the key out.
        Some(g) => out.value("ok", g.get("ok")),
        None => out.raw("ok", "null"),
    }
    out.raw(
        "unavailable",
        bool_json(field("unavailable").is_some_and(truthy)),
    );
    out.raw("at", &or_null(field("at")));
    let steps = records(field("steps")).map(|s| {
        let mut step = JsonObject::new();
        step.value("name", s.get("name"));
        step.value("ok", s.get("ok"));
        step.raw("ms", &or_null(s.get("ms")));
        step.raw("summary", &or_else(s.get("summary"), "\"\""));
        step.finish()
    });
    out.raw("steps", &json_array(steps));
    out.raw("running", &running.map_or_else(null, js_json));
    out.finish()
}

/// The `review` field: `null` unless the verdict record is a plain object.
fn review_json(review: Option<&Value>, fp: &str) -> String {
    let Some(v) = record(review) else {
        return null();
    };
    let mut out = JsonObject::new();
    out.value("verdict", v.get("verdict"));
    out.raw("stale", bool_json(v.get("stale").is_some_and(truthy)));
    out.raw(
        "fingerprint",
        &or_else(v.get("fingerprint"), &json_string(fp)),
    );
    out.raw("recorded_at", &or_null(v.get("recorded_at")));
    out.raw("scores", &or_else(v.get("scores"), "{}"));
    let findings = records(v.get("findings")).map(|f| {
        let mut finding = JsonObject::new();
        finding.value("severity", f.get("severity"));
        finding.value("rule", f.get("rule"));
        finding.value("file", f.get("file"));
        finding.raw("line", &or_null(f.get("line")));
        finding.value("fix", f.get("fix"));
        finding.finish()
    });
    out.raw("findings", &json_array(findings));
    out.finish()
}

/// The `baseline` field: `null`, `{ path, total, toolchain }` or `{ error }`.
fn baseline_json(line: &BaselineLine<'_>) -> String {
    let mut out = JsonObject::new();
    match line {
        BaselineLine::Absent => return null(),
        BaselineLine::Read(b) => {
            out.raw("path", &json_string(BASELINE_FILE));
            out.raw("total", &js_json(b.total()));
            out.raw("toolchain", &or_null(b.toolchain()));
        }
        BaselineLine::Unreadable(error) => out.raw("error", &json_string(&error.to_string())),
    }
    out.finish()
}

/// 1.x `record(x)`: the value when it is a plain object (not `null`, not an array).
fn record(value: Option<&Value>) -> Option<&Object> {
    match value {
        Some(Value::Object(object)) => Some(object),
        _ => None,
    }
}

/// 1.x `list(x)`: the plain objects of an array, or none when `x` is not one.
fn records(value: Option<&Value>) -> impl Iterator<Item = &Object> {
    let items = match value {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    };
    items.iter().filter_map(|item| record(Some(item)))
}

/// `x ?? null` as JSON.
fn or_null(value: Option<&Value>) -> String {
    or_else(value, "null")
}

/// `x ?? fallback` as JSON, `fallback` being JSON already.
fn or_else(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(Value::Null) | None => fallback.to_owned(),
        Some(value) => js_json(value),
    }
}

fn null() -> String {
    "null".to_owned()
}

fn bool_json(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn string_array(items: &[String]) -> String {
    json_array(items.iter().map(|item| json_string(item)))
}

/// `[a,b,…]` of items that are JSON already.
fn json_array(items: impl Iterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
    out
}

/// A JSON object written key by key, in the order the keys are given.
struct JsonObject(String);

impl JsonObject {
    fn new() -> Self {
        Self(String::from("{"))
    }

    /// `key` with `json`, a value written already.
    fn raw(&mut self, key: &str, json: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push_str(&json_string(key));
        self.0.push(':');
        self.0.push_str(json);
    }

    /// `key` with `value`, left out when it is `undefined`, as `JSON.stringify` does.
    fn value(&mut self, key: &str, value: Option<&Value>) {
        if let Some(value) = value {
            self.raw(key, &js_json(value));
        }
    }

    fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

#[cfg(test)]
mod tests;
