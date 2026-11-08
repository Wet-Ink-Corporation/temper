//! Where a package manifest's workspace root is, and whether `.claude/temper.json` lets it carry
//! its own lints (1.x `manifest.mjs` `workspaceRootFor` and `ownLintsMember`).

use std::path::Path;

use temper_domain::{Config, Glob, GlobError, Manifest, OwnLints};

use crate::paths::{self, PathEnv, clean_rel};
use crate::ports::Files;
use crate::project::after_line_start;
use temper_domain::is_js_space;

/// The workspace root that governs a package manifest.
#[derive(Debug)]
pub(crate) enum RootFor {
    /// The manifest has a `[workspace]` table: it is its own root.
    Itself,
    /// A root elsewhere, read from disk.
    Member(Manifest),
}

/// 1.x `workspaceRootFor(manifestPath, manifest)`, following Cargo's rules: the manifest itself
/// when it has a `[workspace]` table, else the root that `package.workspace` names, else the
/// nearest ancestor with a `[workspace]` table, unless that root excludes the package. `None`
/// for a standalone package.
///
/// A root that cannot be read or parsed is passed over, as 1.x's `readText` and `parseToml`
/// give null.
pub(crate) fn workspace_root_for(
    files: &impl Files,
    at: PathEnv<'_>,
    manifest_path: &str,
    manifest: &Manifest,
) -> Option<RootFor> {
    if manifest.has_workspace() {
        return Some(RootFor::Itself);
    }
    let style = at.style;
    let abs = paths::resolve(style, at.cwd, manifest_path);
    let crate_dir = paths::dirname(style, &abs);
    if let Some(explicit) = manifest.package_workspace() {
        // `path.resolve(crateDir, explicit, 'Cargo.toml')`, where an empty `explicit` is skipped.
        let path = if explicit.is_empty() {
            paths::resolve(style, crate_dir, "Cargo.toml")
        } else {
            paths::resolve(style, crate_dir, &[explicit, "/Cargo.toml"].concat())
        };
        let root = read_manifest(files, &path).filter(Manifest::has_workspace)?;
        return Some(RootFor::Member(root));
    }
    let mut dir = paths::dirname(style, crate_dir);
    loop {
        let path = paths::join(style, dir, "Cargo.toml");
        let root = read_text(files, &path)
            .filter(|text| opens_workspace_table(text))
            .and_then(|text| text.parse::<Manifest>().ok())
            .filter(Manifest::has_workspace);
        if let Some(root) = root {
            let rel = clean_rel(&paths::relative(style, at.cwd, dir, crate_dir));
            let excluded = root.workspace_excludes().any(|entry| {
                let x = clean_rel(&entry);
                rel == x
                    || rel
                        .strip_prefix(x.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            });
            return (!excluded).then_some(RootFor::Member(root));
        }
        let parent = paths::dirname(style, dir);
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

/// 1.x `ownLintsMember(config, projectRoot, manifestPath)`: is the manifest's directory,
/// relative to the project root, matched by a `manifests.ownLints` glob?
///
/// # Errors
///
/// [`GlobError`] when an entry does not compile.
pub(crate) fn own_lints_member(
    at: PathEnv<'_>,
    config: &Config,
    project_root: &str,
    manifest_path: &str,
) -> Result<OwnLints, GlobError> {
    let abs = paths::resolve(at.style, at.cwd, manifest_path);
    let dir = clean_rel(&paths::relative(
        at.style,
        at.cwd,
        project_root,
        paths::dirname(at.style, &abs),
    ));
    for entry in config.own_lints() {
        if Glob::new(&clean_rel(entry))?.is_match(&dir) {
            return Ok(OwnLints::Listed);
        }
    }
    Ok(OwnLints::NotListed)
}

/// 1.x `readText(path)`: the text, or `None` when it is missing or cannot be read.
fn read_text(files: &impl Files, path: &str) -> Option<String> {
    match files.read_text(Path::new(path)) {
        Ok(Some(text)) => Some(text),
        // `readText` turns any failure into null, as a missing file is.
        Ok(None) | Err(_) => None,
    }
}

/// 1.x `parseToml(readText(path))`: `None` when the file is missing, unreadable or not TOML.
fn read_manifest(files: &impl Files, path: &str) -> Option<Manifest> {
    read_text(files, path).and_then(|text| text.parse().ok())
}

/// 1.x `/^\s*\[\s*workspace\s*[\].]/m`: a `[workspace]` or `[workspace.<key>]` header.
fn opens_workspace_table(text: &str) -> bool {
    text.match_indices('[').any(|(at, _)| {
        let rest = text.get(at.saturating_add(1)..).unwrap_or("");
        after_line_start(text, at)
            && rest
                .trim_start_matches(is_js_space)
                .strip_prefix("workspace")
                .is_some_and(|tail| tail.trim_start_matches(is_js_space).starts_with([']', '.']))
    })
}

#[cfg(test)]
mod tests;
