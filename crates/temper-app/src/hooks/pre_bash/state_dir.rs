//! Shell writes into temper's state dir (1.x `hooks.mjs` `stateDirWrite`).
//!
//! The state dir is `.git/temper/` under any prefix, the absolute state dir (in a linked
//! worktree, inside the main repository's git dir) with either separator, `target/temper/`
//! without git, and `$VAR/temper/<state entry>`. Only destinations count, as the shell
//! reader finds them: redirections, copy destinations and where each source lands in them,
//! `tee`, `sed -i` and the other writers, inside `bash -c` and compound commands too.

use std::borrow::Cow;

use temper_domain::PathStyle;

use crate::hooks::HookCall;
use crate::hooks::shell::{ParseOpts, PathCx, Target, command_writes, msys_path, parse_commands};
use crate::paths::{self, PathEnv};
use crate::ports::Ports;
use crate::project::Workspace;

/// 1.x `stateDirWrite(proj, cmd, input.cwd ?? proj.root)` under `readerRule`: whether `cmd`
/// writes into `ws`'s state dir. Where 1.x's reader throws, the answer is its fallback, no.
pub(super) fn state_dir_write(call: &HookCall<'_, impl Ports>, ws: &Workspace, cmd: &str) -> bool {
    let env = call.env;
    let style = env.style;
    let cwd = env.cwd.to_string_lossy();
    let start = match call.input.given("cwd").map(|dir| dir.as_str()) {
        None => paths::resolve(style, &cwd, &ws.root),
        Some(Some(dir)) => paths::resolve(style, &cwd, dir),
        // `path.resolve` throws on any other value.
        Some(None) => return false,
    };
    let temp = env.temp_dir.to_string_lossy();
    let home = env.home.to_string_lossy();
    let cx = PathCx {
        at: PathEnv { style, cwd: &cwd },
        os_tmp: &temp,
    };
    let state = paths::resolve(style, &cwd, &ws.state_dir);
    // In a repository, `$(git rev-parse --git-common-dir)` is the state dir's parent.
    let git_dir = if ws.git {
        paths::dirname(style, &state)
    } else {
        ".git"
    };
    let opts = ParseOpts {
        paths: cx,
        cwd: Some(&start),
        home: &home,
        top: Some(&ws.root),
        git_dir,
        tmp: Some(&temp),
    };
    let Ok(commands) = parse_commands(cmd, opts) else {
        return false;
    };
    let probe = Probe {
        cx,
        state: fold(style, slash(&state)),
        git: ws.git,
    };
    commands
        .iter()
        .flat_map(|c| command_writes(c, cx))
        .any(|t| probe.target(&t, &start))
}

/// What a destination is compared with.
struct Probe<'a> {
    cx: PathCx<'a>,
    /// The resolved state dir, with `/` separators, lower case on Windows.
    state: String,
    git: bool,
}

impl Probe<'_> {
    /// A target in the state dir, or a copy whose source lands there (`mv temper .git/`).
    fn target(&self, t: &Target<'_>, start: &str) -> bool {
        let base = t.cwd.as_deref().unwrap_or(start);
        let text = t.word.text();
        let value = t.word.value().filter(|_| !t.word.glob());
        if self.names_state(text, value, base) {
            return true;
        }
        let sources = t.copy.iter().flat_map(|copy| &copy.sources);
        sources.filter_map(|s| s.value()).any(|source| {
            let name = source_name(source);
            if name.is_empty() {
                return false;
            }
            let text = [text, "/", name].concat();
            let value = value.map(|v| [v, "/", name].concat());
            self.names_state(&text, value.as_deref(), base)
        })
    }

    /// 1.x `isState`: either reading of a backslash before a word char counts. PowerShell keeps
    /// it as a separator; bash drops it, and a bash function may carry a cmdlet's name.
    fn names_state(&self, text: &str, value: Option<&str>, base: &str) -> bool {
        if self.names_state_as(text, value, base) {
            return true;
        }
        // 1.x tests the two joined, so a backslash ending the text may escape the value's start.
        let value_or_empty = value.unwrap_or("");
        let across = text.ends_with('\\') && value_or_empty.starts_with(word_or_dash);
        let escaped = has_escape(text) || across || has_escape(value_or_empty);
        escaped
            && self.names_state_as(
                &bash_reading(text),
                value.map(bash_reading).as_deref(),
                base,
            )
    }

    /// 1.x `isStateAs(text, value, base)`.
    fn names_state_as(&self, text: &str, value: Option<&str>, base: &str) -> bool {
        if state_var_entry(&slash(text)) {
            return true;
        }
        let style = self.cx.at.style;
        let named = msys_path(value.unwrap_or(text), style, self.cx.os_tmp);
        let mut p = slash(&named).into_owned();
        if !anchored(&p) {
            p = [slash(base).as_ref(), "/", &p].concat();
        }
        let f = fold(style, Cow::Owned(paths::posix_normalize(&p)));
        if under_entry(&f, ".git/temper") || (!self.git && under_entry(&f, "target/temper")) {
            return true;
        }
        value.is_some()
            && f.strip_prefix(self.state.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

/// 1.x `slash(s)`: backslashes as slashes.
fn slash(s: &str) -> Cow<'_, str> {
    if s.contains('\\') {
        Cow::Owned(s.replace('\\', "/"))
    } else {
        Cow::Borrowed(s)
    }
}

/// 1.x `fold(s)`: lower case on Windows, whose paths ignore case.
fn fold(style: PathStyle, s: Cow<'_, str>) -> String {
    match style {
        PathStyle::Windows => s.to_lowercase(),
        PathStyle::Posix => s.into_owned(),
    }
}

/// A JS `\w`, or `-`.
fn word_or_dash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// `/\\[\w-]/`: a backslash before a word char or `-`.
fn has_escape(s: &str) -> bool {
    s.match_indices('\\').any(|(at, _)| {
        s.get(at.saturating_add(1)..)
            .is_some_and(|r| r.starts_with(word_or_dash))
    })
}

/// 1.x `bashReading(s)`, `s.replace(/\\(?=[\w-])/g, '')`: each backslash before a word char
/// or `-` dropped, and the char kept.
fn bash_reading(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().copied().is_some_and(word_or_dash) {
            continue;
        }
        out.push(c);
    }
    out
}

/// 1.x's `/^\$\{?(\w+)\}?\/temper\/(?:(?:verdicts|gate|logs|sessions)(?:\/|$)|telemetry\.jsonl$|findings-|gate-progress-)/`
/// with the name not one of `OUTSIDE_VARS`: `$VAR/temper/<state entry>`. The name is the
/// whole run of word chars, as no shorter one could be followed by `}` or `/`.
fn state_var_entry(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('$') else {
        return false;
    };
    let rest = rest.strip_prefix('{').unwrap_or(rest);
    let len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    let (name, after) = rest.split_at_checked(len).unwrap_or(("", rest));
    let after = after.strip_prefix('}').unwrap_or(after);
    let Some(entry) = after.strip_prefix("/temper/") else {
        return false;
    };
    let dir = ["verdicts", "gate", "logs", "sessions"].iter().any(|dir| {
        entry
            .strip_prefix(dir)
            .is_some_and(|r| r.is_empty() || r.starts_with('/'))
    });
    let state = dir
        || entry == "telemetry.jsonl"
        || entry.starts_with("findings-")
        || entry.starts_with("gate-progress-");
    !name.is_empty() && state && !outside_var(name)
}

/// 1.x `OUTSIDE_VARS`: a home or temp directory, whose `temper/` is not the state dir.
fn outside_var(name: &str) -> bool {
    const NAMES: [&str; 9] = [
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "PWD",
        "OLDPWD",
    ];
    NAMES.contains(&name)
        || name
            .strip_prefix("XDG_")
            .is_some_and(|rest| !rest.is_empty())
}

/// `/^(?:\/|[a-z]:\/|\$|~)/i`: a path that does not take the base in front of it.
fn anchored(p: &str) -> bool {
    let mut chars = p.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some('/' | '$' | '~'), _, _) => true,
        (Some(drive), Some(':'), Some('/')) => drive.is_ascii_alphabetic(),
        _ => false,
    }
}

/// `(?:^|\/)<entry>(?:\/|$)`: `entry` as whole segments of `f`.
fn under_entry(f: &str, entry: &str) -> bool {
    f.match_indices(entry).any(|(at, _)| {
        let starts = at == 0 || f.get(..at).is_some_and(|before| before.ends_with('/'));
        let ends = f
            .get(at.saturating_add(entry.len())..)
            .is_some_and(|after| after.is_empty() || after.starts_with('/'));
        starts && ends
    })
}

/// 1.x `path.posix.basename(slash(value).replace(/\/+$/, ''))`: the name a source lands as.
fn source_name(value: &str) -> &str {
    // Both separators read as `/`, so the last of either ends the directory part.
    let trimmed = value.trim_end_matches(['/', '\\']);
    trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{anchored, bash_reading, has_escape, source_name, state_var_entry, under_entry};

    /// Each row is what 1.x's expression gives on the same text.
    #[test]
    fn the_state_patterns_read_as_1x_regexes_do() {
        assert_eq!(
            bash_reading(r".git/tem\per/x\ y\-z\\"),
            r".git/temper/x\ y-z\\"
        );
        assert!(has_escape(r"a\-") && !has_escape(r"a\ b\"));
        assert!(state_var_entry("$GIT_DIR/temper/verdicts"));
        assert!(state_var_entry("${D}/temper/gate/x"));
        assert!(state_var_entry("$D}/temper/telemetry.jsonl"));
        assert!(state_var_entry("${D/temper/findings-1"));
        assert!(!state_var_entry("$D/temper/telemetry.jsonl.bak"));
        assert!(!state_var_entry("$D/temper/gateway"));
        assert!(!state_var_entry("$HOME/temper/verdicts"));
        assert!(
            state_var_entry("$XDG_/temper/verdicts") && !state_var_entry("$XDG_X/temper/verdicts")
        );
        assert!(!state_var_entry("${}/temper/verdicts"));
        assert!(anchored("/x") && anchored("C:/x") && anchored("$X") && anchored("~u"));
        assert!(!anchored("C:x") && !anchored("x"));
        assert!(
            under_entry("/r/.git/temper", ".git/temper")
                && under_entry(".git/temper/x", ".git/temper")
        );
        assert!(
            !under_entry("/r/x.git/temper", ".git/temper")
                && !under_entry("/r/.git/tempers", ".git/temper")
        );
        assert!(under_entry("/r/x.git/temper/.git/temper", ".git/temper"));
        assert_eq!(
            [source_name("a/b//"), source_name(r"a\b"), source_name("/")],
            ["b", "b", ""]
        );
    }
}
