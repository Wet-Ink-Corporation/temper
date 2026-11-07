//! What the gate reads from `.claude/temper.json`, as canonical JSON (1.x `gateConfig`).

use crate::js::{Object, Value, canonical};

/// The sections the gate reads and their defaults, every property the gate reads: 1.x's
/// `GATE_CONFIG_KEYS` cut from `DEFAULT_CONFIG`. Both levels are listed in sorted order, the
/// order `canonical` gives them, so only the values taken from the file need sorting.
fn defaults() -> [(&'static str, Vec<(&'static str, Value)>); 3] {
    [
        (
            "gate",
            vec![
                ("allFeatures", Value::Bool(true)),
                ("doc", Value::Bool(true)),
                ("scan", Value::Bool(true)),
                ("stepTimeoutSec", Value::Number(900.0)),
                ("steps", Value::Null),
            ],
        ),
        ("manifests", vec![("ownLints", Value::Array(Vec::new()))]),
        ("scanner", vec![("exempt", Value::Array(Vec::new()))]),
    ]
}

/// 1.x `gateConfig(text)`: what the gate reads from the config text, as canonical JSON.
///
/// The `gate`, `scanner` and `manifests` sections are each merged over their defaults and cut
/// to the properties the defaults list, with keys sorted at every depth. It reads the raw JSON,
/// not [`Config`](crate::Config): a value of the wrong type is kept as it is, as 1.x keeps it.
///
/// No text, empty text, text that is not JSON (D16(n), OC-3: also what `serde_json` rejects
/// and V8 reads) and a document that is not an object read as `{}`. A section that is not an
/// object adds nothing, since spreading a string adds only index keys and the gate reads none.
///
/// # Examples
///
/// ```
/// use temper_domain::gate_config;
///
/// assert_eq!(gate_config(Some(r#"{"review":"off"}"#)), gate_config(None));
/// assert_ne!(gate_config(Some(r#"{"gate":{"doc":false}}"#)), gate_config(None));
/// ```
#[must_use]
pub fn gate_config(text: Option<&str>) -> String {
    let user = match text.filter(|text| !text.is_empty()).map(Value::from_json) {
        Some(Ok(user)) => Some(user),
        // 1.x's `catch { /* loadConfig falls back to the defaults */ }`.
        Some(Err(_)) | None => None,
    };
    let sections: Object = defaults()
        .into_iter()
        .map(|(key, props)| {
            let given = user.as_ref().and_then(|user| user.get(key));
            let picked: Object = props
                .into_iter()
                .map(|(prop, default)| {
                    let value = given.and_then(|given| given.get(prop));
                    (prop.to_owned(), value.map_or(default, canonical))
                })
                .collect();
            (key.to_owned(), Value::Object(picked))
        })
        .collect();
    Value::Object(sections).to_json()
}

#[cfg(test)]
mod tests;
