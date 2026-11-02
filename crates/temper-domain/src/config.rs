//! temper's project config, `.claude/temper.json`, as 1.x's `loadConfig` merges it over
//! `DEFAULT_CONFIG`: top-level keys replace, and `gate`, `scanner`, `manifests` and `telemetry`
//! merge one level.
//!
//! Globs stay pattern strings here: the application compiles them where it matches (port plan
//! 2-8), so loading cannot fail. What 1.x silently ignored, an unreadable file or a key of the
//! wrong type, is reported beside the defaults that replace it (D16(d), OC-2 B).

use std::fmt;

use serde_json::{Map, Value};

mod defaults;

/// The merged config. Every getter answers what 1.x's readers see.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    skill_gate: bool,
    review: ReviewMode,
    ask_on_new_dependency: bool,
    preapproved_dependencies: Vec<String>,
    gate: GateConfig,
    protected: Vec<String>,
    exempt: Vec<ExemptRule>,
    own_lints: Vec<String>,
    telemetry_branch: Option<String>,
}

/// How the Stop hook treats review: `review` in the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    /// `"required"`: an approved review is needed before stopping.
    Required,
    /// `"advisory"`: a missing review is mentioned, not enforced.
    Advisory,
    /// `"off"`: review is not checked.
    Off,
}

/// The gate's settings: `gate` in the config.
#[derive(Debug, Clone, PartialEq)]
pub struct GateConfig {
    features: CargoFeatures,
    doc: Switch,
    scan: Switch,
    step_timeout_sec: f64,
}

/// Which features the gate's cargo steps build: `allFeatures`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoFeatures {
    /// `true`: pass `--all-features`.
    All,
    /// `false`: the default features.
    Default,
}

/// Whether an optional gate step runs: `doc` and `scan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// `true`: the step runs.
    On,
    /// `false`: the step is skipped.
    Off,
}

impl Switch {
    const fn of(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

impl GateConfig {
    /// `allFeatures`.
    #[must_use]
    pub const fn features(&self) -> CargoFeatures {
        self.features
    }

    /// `doc`: the rustdoc step.
    #[must_use]
    pub const fn doc(&self) -> Switch {
        self.doc
    }

    /// `scan`: temper's escape-hatch scan.
    #[must_use]
    pub const fn scan(&self) -> Switch {
        self.scan
    }

    /// `stepTimeoutSec`: each step's timeout, in seconds, as the JSON number gives it.
    #[must_use]
    pub const fn step_timeout_sec(&self) -> f64 {
        self.step_timeout_sec
    }
}

/// One `scanner.exempt` entry: a rule the scanner does not report under some paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExemptRule {
    rule: String,
    paths: Vec<String>,
}

impl ExemptRule {
    /// The rule id, such as `H-01`.
    #[must_use]
    pub fn rule(&self) -> &str {
        &self.rule
    }

    /// The globs it applies under (none when `paths` is absent or `null`).
    #[must_use]
    pub fn paths(&self) -> &[String] {
        &self.paths
    }
}

/// The config, and what was wrong with the file when something was.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLoad {
    /// The merged config; every unreadable part takes its default.
    pub config: Config,
    /// The first problem, in 1.x's key order, for `status` and the `SessionStart` context.
    pub problem: Option<ConfigProblem>,
}

/// Why (part of) the config was replaced by defaults. `Display` is the `<why>` of D16(d)'s
/// "config unreadable (`<why>`): defaults in use".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigProblem {
    /// `serde_json` cannot read the file; the detail is its message (D16(a)). That covers text
    /// that is not JSON and numbers out of `f64` range such as `1e400`, which V8 reads as
    /// `Infinity` (D16(d)).
    NotJson {
        /// The parser's message.
        detail: String,
    },
    /// The file is JSON but not an object.
    NotAnObject,
    /// A key has the wrong type, so it takes its default (OC-2 B).
    WrongType {
        /// The key's path, such as `gate.doc`.
        key: &'static str,
        /// What it should be, such as `a boolean`.
        expected: &'static str,
    },
}

impl fmt::Display for ConfigProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson { detail } => write!(f, "unreadable JSON: {detail}"),
            Self::NotAnObject => f.write_str("not a JSON object"),
            Self::WrongType { key, expected } => write!(f, "`{key}` is not {expected}"),
        }
    }
}

const A_BOOLEAN: &str = "a boolean";
const STRINGS: &str = "an array of strings";
const AN_OBJECT: &str = "an object";
const EXEMPTS: &str = "an array of { rule, paths } objects";
const REVIEW_MODES: &str = "\"required\", \"advisory\" or \"off\"";

impl Config {
    /// 1.x's `DEFAULT_CONFIG`.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            skill_gate: true,
            review: ReviewMode::Required,
            ask_on_new_dependency: true,
            preapproved_dependencies: defaults::strings(&defaults::PREAPPROVED_DEPENDENCIES),
            gate: GateConfig {
                features: CargoFeatures::All,
                doc: Switch::On,
                scan: Switch::On,
                step_timeout_sec: defaults::STEP_TIMEOUT_SEC,
            },
            protected: defaults::strings(&defaults::PROTECTED),
            exempt: Vec::new(),
            own_lints: Vec::new(),
            telemetry_branch: None,
        }
    }

    /// Loads the text of `.claude/temper.json` (`None` when there is no file). No file and an
    /// empty one give the defaults, as 1.x's `if (!text)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use temper_domain::{Config, ConfigProblem};
    ///
    /// let load = Config::load(Some(r#"{"skillGate": false}"#));
    /// assert!(!load.config.skill_gate());
    /// assert_eq!(load.problem, None);
    ///
    /// let load = Config::load(Some("{"));
    /// assert_eq!(load.config, Config::defaults());
    /// assert!(matches!(load.problem, Some(ConfigProblem::NotJson { .. })));
    /// ```
    #[must_use]
    pub fn load(text: Option<&str>) -> ConfigLoad {
        let defaults = || ConfigLoad {
            config: Self::defaults(),
            problem: None,
        };
        let Some(text) = text.filter(|t| !t.is_empty()) else {
            return defaults();
        };
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(user)) => {
                let mut reader = Reader::default();
                let config = reader.config(&user);
                ConfigLoad {
                    config,
                    problem: reader.problem,
                }
            }
            Ok(_) => ConfigLoad {
                problem: Some(ConfigProblem::NotAnObject),
                ..defaults()
            },
            Err(e) => ConfigLoad {
                problem: Some(ConfigProblem::NotJson {
                    detail: e.to_string(),
                }),
                ..defaults()
            },
        }
    }

    /// `skillGate`: Rust edits wait for the rust-craft skill.
    #[must_use]
    pub const fn skill_gate(&self) -> bool {
        self.skill_gate
    }

    /// `review`.
    #[must_use]
    pub const fn review(&self) -> ReviewMode {
        self.review
    }

    /// `askOnNewDependency`: a new dependency asks unless it is pre-approved.
    #[must_use]
    pub const fn ask_on_new_dependency(&self) -> bool {
        self.ask_on_new_dependency
    }

    /// `preapprovedDependencies`.
    #[must_use]
    pub fn preapproved_dependencies(&self) -> &[String] {
        &self.preapproved_dependencies
    }

    /// `gate`.
    #[must_use]
    pub const fn gate(&self) -> &GateConfig {
        &self.gate
    }

    /// `protected`: globs of verification and agent configuration.
    #[must_use]
    pub fn protected(&self) -> &[String] {
        &self.protected
    }

    /// `scanner.exempt`.
    #[must_use]
    pub fn exempt(&self) -> &[ExemptRule] {
        &self.exempt
    }

    /// `manifests.ownLints`: member directories (globs) that carry their own `[lints]`.
    #[must_use]
    pub fn own_lints(&self) -> &[String] {
        &self.own_lints
    }

    /// `telemetry.branch`: where Stop pushes the session's telemetry; an empty name means none,
    /// as 1.x's `!branch` reads it.
    #[must_use]
    pub fn telemetry_branch(&self) -> Option<&str> {
        self.telemetry_branch.as_deref()
    }
}

/// What a `null` list means to 1.x's reader: `?? []` or `?? DEFAULT`.
#[derive(Clone, Copy)]
enum NullList {
    Empty,
    Default,
}

/// A list-of-strings key: its path in problems, and what `null` means for it.
#[derive(Clone, Copy)]
struct ListKey {
    path: &'static str,
    null: NullList,
}

const PREAPPROVED_KEY: ListKey = ListKey {
    path: "preapprovedDependencies",
    null: NullList::Empty,
};
const PROTECTED_KEY: ListKey = ListKey {
    path: "protected",
    null: NullList::Default,
};
const OWN_LINTS_KEY: ListKey = ListKey {
    path: "manifests.ownLints",
    null: NullList::Empty,
};

/// Reads the user's keys over the defaults, keeping the first problem.
#[derive(Default)]
struct Reader {
    problem: Option<ConfigProblem>,
}

impl Reader {
    fn wrong<T>(&mut self, key: &'static str, expected: &'static str, default: T) -> T {
        self.problem
            .get_or_insert(ConfigProblem::WrongType { key, expected });
        default
    }

    fn config(&mut self, user: &Map<String, Value>) -> Config {
        let d = Config::defaults();
        // In `DEFAULT_CONFIG`'s key order, so the first problem is the first key 1.x lists.
        let skill_gate = self.boolean(user.get("skillGate"), "skillGate", d.skill_gate);
        let review = self.review(user.get("review"), d.review);
        let ask = user.get("askOnNewDependency");
        let ask_on_new_dependency =
            self.boolean(ask, "askOnNewDependency", d.ask_on_new_dependency);
        let preapproved = user.get("preapprovedDependencies");
        let preapproved_dependencies =
            self.strings(preapproved, PREAPPROVED_KEY, d.preapproved_dependencies);
        let gate = self.gate(user.get("gate"), d.gate);
        let protected = self.strings(user.get("protected"), PROTECTED_KEY, d.protected);
        let exempt = match self.section(user.get("scanner"), "scanner") {
            Some(scanner) => self.exempt(scanner.get("exempt")),
            None => d.exempt,
        };
        let own_lints = match self.section(user.get("manifests"), "manifests") {
            Some(m) => self.strings(m.get("ownLints"), OWN_LINTS_KEY, d.own_lints),
            None => d.own_lints,
        };
        let telemetry = self.section(user.get("telemetry"), "telemetry");
        let telemetry_branch = self.branch(telemetry.and_then(|t| t.get("branch")));
        Config {
            skill_gate,
            review,
            ask_on_new_dependency,
            preapproved_dependencies,
            gate,
            protected,
            exempt,
            own_lints,
            telemetry_branch,
        }
    }

    fn review(&mut self, value: Option<&Value>, default: ReviewMode) -> ReviewMode {
        match value.map(|v| v.as_str()) {
            None => default,
            Some(Some("required")) => ReviewMode::Required,
            Some(Some("advisory")) => ReviewMode::Advisory,
            Some(Some("off")) => ReviewMode::Off,
            Some(_) => self.wrong("review", REVIEW_MODES, default),
        }
    }

    /// `telemetry.branch`: a string or `null`; an empty name means none.
    fn branch(&mut self, value: Option<&Value>) -> Option<String> {
        match value {
            None | Some(Value::Null) => None,
            // The config owns its values; the parsed document is dropped after loading.
            Some(Value::String(branch)) => Some(branch.clone()).filter(|b| !b.is_empty()),
            Some(_) => self.wrong("telemetry.branch", "a string or null", None),
        }
    }

    fn boolean(&mut self, value: Option<&Value>, key: &'static str, default: bool) -> bool {
        match value {
            None => default,
            Some(Value::Bool(b)) => *b,
            Some(_) => self.wrong(key, A_BOOLEAN, default),
        }
    }

    /// A one-level merged section: absent or `null` reads as `{}` (1.x's `?? {}`).
    fn section<'v>(
        &mut self,
        value: Option<&'v Value>,
        key: &'static str,
    ) -> Option<&'v Map<String, Value>> {
        match value {
            None | Some(Value::Null) => None,
            Some(Value::Object(section)) => Some(section),
            Some(_) => self.wrong(key, AN_OBJECT, None),
        }
    }

    fn strings(
        &mut self,
        value: Option<&Value>,
        key: ListKey,
        default: Vec<String>,
    ) -> Vec<String> {
        match value {
            None => default,
            Some(Value::Null) => match key.null {
                NullList::Empty => Vec::new(),
                NullList::Default => default,
            },
            Some(Value::Array(items)) => match string_items(items) {
                Some(list) => list,
                None => self.wrong(key.path, STRINGS, default),
            },
            Some(_) => self.wrong(key.path, STRINGS, default),
        }
    }

    fn gate(&mut self, value: Option<&Value>, default: GateConfig) -> GateConfig {
        let Some(gate) = self.section(value, "gate") else {
            return default;
        };
        let step_timeout_sec = match gate.get("stepTimeoutSec") {
            None => default.step_timeout_sec,
            Some(n) => match n.as_f64() {
                Some(secs) => secs,
                None => self.wrong("gate.stepTimeoutSec", "a number", default.step_timeout_sec),
            },
        };
        let all = default.features == CargoFeatures::All;
        let all = self.boolean(gate.get("allFeatures"), "gate.allFeatures", all);
        let doc = self.boolean(gate.get("doc"), "gate.doc", default.doc == Switch::On);
        let scan = self.boolean(gate.get("scan"), "gate.scan", default.scan == Switch::On);
        GateConfig {
            features: if all {
                CargoFeatures::All
            } else {
                CargoFeatures::Default
            },
            doc: Switch::of(doc),
            scan: Switch::of(scan),
            step_timeout_sec,
        }
    }

    fn exempt(&mut self, value: Option<&Value>) -> Vec<ExemptRule> {
        let rules = match value {
            None | Some(Value::Null) => Some(Vec::new()),
            Some(Value::Array(items)) => items.iter().map(exempt_rule).collect(),
            Some(_) => None,
        };
        rules.unwrap_or_else(|| self.wrong("scanner.exempt", EXEMPTS, Vec::new()))
    }
}

/// Every item as a string, or `None` when one is not.
fn string_items(items: &[Value]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect()
}

/// `{ rule: string, paths?: string[] | null }`.
fn exempt_rule(item: &Value) -> Option<ExemptRule> {
    let rule = item.get("rule")?.as_str()?.to_owned();
    let paths = match item.get("paths") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => string_items(items)?,
        Some(_) => return None,
    };
    Some(ExemptRule { rule, paths })
}

#[cfg(test)]
mod tests;
