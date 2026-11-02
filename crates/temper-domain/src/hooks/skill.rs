//! When the rust-craft skill counts as loaded (1.x `hooks.mjs` `postTool` and `userPromptSubmit`).

/// `user-prompt`'s context while the skill is not loaded in the session.
pub const SKILL_NOT_LOADED: &str = "temper: this is a Rust workspace and the rust-craft standard (skill `temper:rust-craft`) is not loaded in this session yet; Rust edits are refused until it is. Planning Rust work benefits from loading it first.";

/// The tail of the skill's `SKILL.md`, with `/` separators.
const SKILL_DOC: &[u8] = b"/rust-craft/SKILL.md";

/// How the skill was loaded, as telemetry's `via` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillVia {
    /// The `Skill` tool.
    Skill,
    /// A `Read` of its `SKILL.md`.
    Read,
}

impl SkillVia {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Read => "Read",
        }
    }
}

/// Whether a finished tool call loaded the skill, and how.
///
/// It did through `Skill` with a `skill` (or `name`) of `rust-craft` or `<plugin>:rust-craft`,
/// or through `Read` of a path ending `/rust-craft/SKILL.md`, with either separator. `skill` and
/// `file_path` are the tool input's fields as JS's `String(x ?? '')` reads them.
#[must_use]
pub fn skill_load_via(tool_name: Option<&str>, skill: &str, file_path: &str) -> Option<SkillVia> {
    match tool_name {
        // 1.x `/(^|:)rust-craft$/`.
        Some("Skill") if skill == "rust-craft" || skill.ends_with(":rust-craft") => {
            Some(SkillVia::Skill)
        }
        // 1.x `/\/rust-craft\/SKILL\.md$/` on the path with `\` read as `/`.
        Some("Read") if is_skill_doc(file_path) => Some(SkillVia::Read),
        _ => None,
    }
}

/// Does an agent of `agent_type` carry the skill preloaded?
///
/// temper's own agents do: `rust-implementer`, `rust-reviewer` and `rust-architect`, bare or
/// `<plugin>:`-prefixed (1.x `PRELOADED` on `String(agent_type ?? '')`).
#[must_use]
pub fn skill_preloaded(agent_type: &str) -> bool {
    ["rust-implementer", "rust-reviewer", "rust-architect"]
        .iter()
        .any(|name| {
            agent_type
                .strip_suffix(name)
                .is_some_and(|head| head.is_empty() || head.ends_with(':'))
        })
}

fn is_skill_doc(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes
        .get(bytes.len().saturating_sub(SKILL_DOC.len())..)
        .is_some_and(|tail| {
            tail.len() == SKILL_DOC.len()
                && tail
                    .iter()
                    .map(|&b| if b == b'\\' { b'/' } else { b })
                    .eq(SKILL_DOC.iter().copied())
        })
}
