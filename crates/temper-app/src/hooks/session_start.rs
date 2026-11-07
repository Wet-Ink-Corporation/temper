//! `session-start` (1.x `hooks.mjs` `sessionStart`).

use std::path::Path;

use temper_domain::{EnvExports, PathStyle, env_file_lines};

use crate::engine::RunEnv;
use crate::paths;
use crate::ports::{Files, Ports};

/// 1.x `cli()`: `path.join(pluginRoot(), 'scripts', 'temper.mjs')`.
fn plugin_cli(style: PathStyle, root: &str) -> String {
    let tail = match style {
        PathStyle::Posix => "scripts/temper.mjs",
        PathStyle::Windows => r"scripts\temper.mjs",
    };
    if root.is_empty() {
        // `path.join` skips an empty part.
        return tail.to_owned();
    }
    if paths::is_absolute(style, root) {
        // For an absolute root, `path.join` normalises as `path.resolve` does.
        return paths::resolve(style, root, tail);
    }
    let joined = paths::posix_normalize(&format!("{}/{tail}", paths::norm(root)));
    match style {
        PathStyle::Posix => joined,
        PathStyle::Windows => joined.replace('/', r"\"),
    }
}

/// The 1.x CLI under the plugin root, when there is one.
fn cli_path(env: &RunEnv) -> Option<String> {
    let root = env.plugin_root.as_deref()?;
    Some(plugin_cli(env.style, &root.to_string_lossy()))
}

/// 1.x `gateCmd()`: `node "<cli>" gate`; `temper gate` for the binary run without a plugin
/// root (OC-6 A).
fn gate_command(env: &RunEnv) -> String {
    match cli_path(env) {
        Some(cli) => format!("node \"{cli}\" gate"),
        None => "temper gate".to_owned(),
    }
}

/// Appends the session's exports to `CLAUDE_ENV_FILE` in one write (1.x's `appendFileSync`,
/// then D17's lines, OC-6 A); whether it was written. Without the variable, or with nothing to
/// export, nothing is; a write that fails is ignored, as 1.x's `catch { /* ignore */ }`.
fn write_env_file(ports: &impl Ports, env: &RunEnv, base: Option<&str>) -> bool {
    let Some(file) = env.env_file.as_deref() else {
        return false;
    };
    let cli = cli_path(env).map(|cli| paths::norm(&cli).into_owned());
    let bin = env
        .temper_bin
        .as_deref()
        .map(|bin| bin.to_string_lossy().into_owned());
    let bin_dir = bin.as_deref().map(|bin| paths::dirname(env.style, bin));
    let text = env_file_lines(&EnvExports {
        cli: cli.as_deref(),
        base,
        bin: bin.as_deref(),
        bin_dir,
        style: env.style,
    });
    // `append_line` adds the last newline back.
    match text.strip_suffix('\n') {
        Some(lines) => ports.files().append_line(Path::new(file), lines).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
