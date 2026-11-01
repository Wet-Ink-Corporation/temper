//! `temper scan --all` and `temper scan FILES..` (1.x `temper.mjs` `scan` and `walk`).

use std::path::Path;

use temper_domain::{
    CodeKind, FileRole, Hit, HitAt, NOT_A_RUST_PROJECT, PathStyle, ScanOptions, Scanner, Severity,
    classify_path, is_build_script, render_scan_text,
};

use crate::engine::{
    CommandError, CommandOutput, Exit, OutputFormat, RunEnv, ScanArgs, ScanTargets,
};
use crate::paths::{self, PathEnv};
use crate::ports::{EntryKind, Files, FsError, Ports};
use crate::project::{crate_of, discover};
use crate::relpath::rel_to;

/// The names the walk neither enters nor lists, at any depth.
const SKIPPED: [&str; 4] = ["target", ".git", "node_modules", ".temper"];

/// A file that has hits, with its path relative to the project root.
struct ScannedFile {
    file: String,
    hits: Vec<Hit>,
}

/// The body of [`Engine::scan`](crate::Engine::scan).
pub(crate) fn run<P: Ports>(
    ports: &P,
    env: &RunEnv,
    scanner: &Scanner,
    args: &ScanArgs,
) -> Result<CommandOutput, CommandError> {
    let files = ports.files();
    let Some(project) = discover(files, env) else {
        return Err(CommandError::Refused {
            exit: Exit::Usage,
            message: NOT_A_RUST_PROJECT.to_owned(),
        });
    };
    let cwd = env.cwd.to_string_lossy();
    let place = Place {
        env: PathEnv {
            style: env.style,
            cwd: &cwd,
        },
        root: &project.root,
    };
    let targets = match &args.targets {
        ScanTargets::Files(names) => names
            .iter()
            .map(|name| paths::resolve(env.style, &cwd, name))
            .collect(),
        ScanTargets::All => walk(files, env.style, &project.root)?,
    };
    let results: Vec<ScannedFile> = targets
        .iter()
        .filter_map(|abs| scan_file(files, scanner, &place, abs))
        .collect();
    let hits: Vec<HitAt<'_>> = results
        .iter()
        .flat_map(|result| {
            let file = result.file.as_str();
            result.hits.iter().map(move |hit| HitAt { hit, file })
        })
        .collect();
    let exit = if hits.iter().any(|at| at.hit.severity() == Severity::Hard) {
        Exit::Fail
    } else {
        Exit::Ok
    };
    let stdout = match args.format {
        OutputFormat::Json => {
            let mut json = serde_json::to_string_pretty(&hits).map_err(CommandError::Json)?;
            json.push('\n');
            json
        }
        OutputFormat::Text => render_scan_text(hits),
    };
    Ok(CommandOutput {
        stdout,
        stderr: String::new(),
        exit,
    })
}

/// Where paths are resolved from, and reported relative to.
struct Place<'a> {
    env: PathEnv<'a>,
    root: &'a str,
}

/// Scans one target as 1.x does: classified by its crate, reported relative to the root.
/// `None` when it cannot be read or has no hits.
fn scan_file(
    files: &impl Files,
    scanner: &Scanner,
    place: &Place<'_>,
    abs: &str,
) -> Option<ScannedFile> {
    // 1.x's `readText` turns any failure into null, and `scan` skips the file.
    let Ok(Some(src)) = files.read_text(Path::new(abs)) else {
        return None;
    };
    let krate = crate_of(files, place.env, abs);
    let base = krate
        .as_ref()
        .map_or(place.root, |krate| krate.dir.as_str());
    let crate_rel = rel_to(files, place.env, base, abs);
    let options = ScanOptions {
        kind: krate.map_or(CodeKind::Prod, |krate| classify_path(&crate_rel, krate.lib)),
        file: if is_build_script(&crate_rel) {
            FileRole::BuildScript
        } else {
            FileRole::Source
        },
        forbid: None,
    };
    let hits = scanner.scan(&src, &options).into_hits();
    if hits.is_empty() {
        return None;
    }
    Some(ScannedFile {
        file: rel_to(files, place.env, place.root, abs),
        hits,
    })
}

/// 1.x `walk(root)`: every `.rs` path below `root`, depth-first in listing order, skipping
/// [`SKIPPED`] names and never entering a symlinked directory. An explicit stack, so a deep
/// tree cannot overflow the call stack.
///
/// Names are decoded lossily as Node decodes them, so a non-UTF-8 directory cannot be listed
/// (1.x fails the same way) and a non-UTF-8 file cannot be read (and is skipped).
pub(crate) fn walk(
    files: &impl Files,
    style: PathStyle,
    root: &str,
) -> Result<Vec<String>, FsError> {
    let mut found = Vec::new();
    let mut stack = vec![(
        root.to_owned(),
        files.read_dir(Path::new(root))?.into_iter(),
    )];
    while let Some((dir, entries)) = stack.last_mut() {
        let Some(entry) = entries.next() else {
            stack.pop();
            continue;
        };
        let name = entry.name.to_string_lossy();
        if SKIPPED.contains(&name.as_ref()) {
            continue;
        }
        let path = paths::join(style, dir, &name);
        match entry.kind {
            EntryKind::Dir => {
                let listing = files.read_dir(Path::new(&path))?;
                stack.push((path, listing.into_iter()));
            }
            EntryKind::File | EntryKind::Symlink | EntryKind::Other => {
                if name.ends_with(".rs") {
                    found.push(path);
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
