//! Node's `path.win32`: `resolve`, `relative`, `isAbsolute` and the root parser they share.

use std::borrow::Cow;

use super::normalize_string;

/// `\` as a UTF-16 code unit, for `relative`'s unit-by-unit comparison.
const BACKSLASH: u16 = 0x5C;

fn is_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

/// A path's root as Node's `win32.resolve` parses it.
struct Root {
    /// `C:`, `\\host\share` (always with `\`) or a device namespace such as `\\.`; empty when
    /// there is none.
    device: String,
    /// The byte length of the root in the path.
    end: usize,
    absolute: bool,
}

fn parse_root(path: &str) -> Root {
    let mut root = Root {
        device: String::new(),
        end: 0,
        absolute: false,
    };
    match path.as_bytes() {
        [only] if is_sep(*only) => {
            root.end = 1;
            root.absolute = true;
        }
        [first, second, ..] if is_sep(*first) => {
            root.absolute = true;
            if is_sep(*second) {
                if let Some((device, end)) = unc_root(path) {
                    root.device = device;
                    root.end = end;
                }
            } else {
                root.end = 1;
            }
        }
        [letter, b':', rest @ ..] if letter.is_ascii_alphabetic() => {
            // Two ASCII bytes, so 2 is a char boundary.
            path.get(..2).unwrap_or("").clone_into(&mut root.device);
            root.end = 2;
            if rest.first().copied().is_some_and(is_sep) {
                root.absolute = true;
                root.end = 3;
            }
        }
        _ => {}
    }
    root
}

/// The device and root length of a path that starts with two separators: `\\host\share` up to
/// the share's end, or `\\.` and `\\?` with length 4. `None` without a host and a share.
fn unc_root(path: &str) -> Option<(String, usize)> {
    let bytes = path.as_bytes();
    let run = |from: usize, sep: bool| {
        bytes
            .get(from..)
            .map_or(0, |rest| {
                rest.iter().take_while(|&&b| is_sep(b) == sep).count()
            })
            .saturating_add(from)
    };
    let host_end = run(2, false);
    if host_end >= bytes.len() || host_end == 2 {
        return None;
    }
    let share_start = run(host_end, true);
    if share_start >= bytes.len() {
        return None;
    }
    let share_end = run(share_start, false);
    // Every bound sits next to an ASCII separator, so each is a char boundary.
    let host = path.get(2..host_end)?;
    if host == "." || host == "?" {
        return Some((["\\\\", host].concat(), 4));
    }
    let share = path.get(share_start..share_end)?;
    Some((["\\\\", host, "\\", share].concat(), share_end))
}

/// What `resolve` has gathered so far, folding paths from the last to the first.
struct Resolved {
    device: String,
    tail: String,
    absolute: bool,
}

impl Resolved {
    /// Folds in one more path (an earlier argument, then the working directory). `true` when
    /// the result is complete.
    fn take(&mut self, path: &str) -> bool {
        let root = parse_root(path);
        if !root.device.is_empty() {
            if self.device.is_empty() {
                self.device = root.device;
            } else if root.device.to_lowercase() != self.device.to_lowercase() {
                // Another device's path does not apply.
                return false;
            }
        }
        if self.absolute {
            return !self.device.is_empty();
        }
        self.tail = [path.get(root.end..).unwrap_or(""), "\\", &self.tail].concat();
        self.absolute = root.absolute;
        root.absolute && !self.device.is_empty()
    }

    fn finish(self) -> String {
        let tail = normalize_string(&self.tail, !self.absolute, '\\', is_sep);
        if self.absolute {
            [self.device.as_str(), "\\", &tail].concat()
        } else if self.device.is_empty() && tail.is_empty() {
            ".".to_owned()
        } else {
            [self.device, tail].concat()
        }
    }
}

/// Node's `win32.resolve(path)` with `process.cwd()` = `cwd`.
pub(super) fn resolve(cwd: &str, path: &str) -> String {
    let mut resolved = Resolved {
        device: String::new(),
        tail: String::new(),
        absolute: false,
    };
    if !path.is_empty() && resolved.take(path) {
        return resolved.finish();
    }
    let base = if resolved.device.is_empty() {
        if (path.is_empty() || path == ".") && cwd.bytes().next().is_some_and(is_sep) {
            return cwd.to_owned();
        }
        Cow::Borrowed(cwd)
    } else {
        drive_cwd(&resolved.device, cwd)
    };
    resolved.take(&base);
    resolved.finish()
}

/// The working directory of `device` for a drive-relative path such as `D:x`. Node reads the
/// `=D:` variable, which only `cmd.exe` sets, then falls back to `process.cwd()`.
fn drive_cwd<'a>(_device: &str, cwd: &'a str) -> Cow<'a, str> {
    Cow::Borrowed(cwd)
}

/// Node's `win32.isAbsolute(path)`.
pub(super) fn is_absolute(path: &str) -> bool {
    match path.as_bytes() {
        [first, ..] if is_sep(*first) => true,
        [letter, b':', sep, ..] => letter.is_ascii_alphabetic() && is_sep(*sep),
        _ => false,
    }
}

/// The byte length of a resolved path's root, the separator after it included: `C:\`, `\` or
/// `\\host\share\` (as Node's `dirname` sees it).
pub(super) fn root_len(path: &str) -> usize {
    let root = parse_root(path);
    let unc = root.device.starts_with("\\\\");
    match path.as_bytes().get(root.end) {
        Some(&b) if unc && is_sep(b) => root.end.saturating_add(1),
        _ => root.end,
    }
}

/// Node's `win32.relative(from, to)` with `process.cwd()` = `cwd`: case-insensitive, keeping
/// `to`'s case.
pub(super) fn relative(cwd: &str, from: &str, to: &str) -> String {
    if from == to {
        return String::new();
    }
    let from_orig = resolve(cwd, from);
    let to_orig = resolve(cwd, to);
    if from_orig == to_orig {
        return String::new();
    }
    let (from_lower, to_lower) = (from_orig.to_lowercase(), to_orig.to_lowercase());
    if from_lower == to_lower {
        return String::new();
    }
    // Node indexes UTF-16 code units, and the lowered strings index the original `to` only
    // while lowering keeps their length.
    let units = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let (from16, to16) = (units(&from_lower), units(&to_lower));
    let to_orig16 = units(&to_orig);
    if units(&from_orig).len() != from16.len() || to_orig16.len() != to16.len() {
        return segment_relative(&from_orig, &to_orig);
    }
    unit_relative(&from16, &to16, &to_orig16)
}

/// Node's `relative` when lowering changed a length: segment by segment.
fn segment_relative(from_orig: &str, to_orig: &str) -> String {
    let split = |s: &str| {
        let mut parts: Vec<String> = s.split('\\').map(str::to_owned).collect();
        if parts.last().is_some_and(String::is_empty) {
            parts.pop();
        }
        parts
    };
    let (from, to) = (split(from_orig), split(to_orig));
    let length = from.len().min(to.len());
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a.to_lowercase() == b.to_lowercase())
        .count();
    let rest = to.get(common..).unwrap_or(&[]).join("\\");
    if common == 0 {
        to_orig.to_owned()
    } else if common < length {
        ["..\\".repeat(from.len().saturating_sub(common)), rest].concat()
    } else if to.len() > length {
        rest
    } else if from.len() > length {
        [
            "..\\".repeat(from.len().saturating_sub(1).saturating_sub(common)),
            "..".to_owned(),
        ]
        .concat()
    } else {
        String::new()
    }
}

/// Node's `relative` on the lowered UTF-16 units of `from` and `to`, slicing `to_orig`.
fn unit_relative(from: &[u16], to: &[u16], to_orig: &[u16]) -> String {
    let at = |v: &[u16], k: usize| v.get(k).copied();
    let slice =
        |start: usize, end: usize| String::from_utf16_lossy(to_orig.get(start..end).unwrap_or(&[]));
    let (from_start, from_end) = trimmed(from);
    let (mut to_start, to_end) = trimmed(to);
    let (from_len, to_len) = (
        from_end.saturating_sub(from_start),
        to_end.saturating_sub(to_start),
    );
    let length = from_len.min(to_len);
    let mut last_sep = None;
    let mut i = 0;
    while i < length {
        let unit = at(from, from_start.saturating_add(i));
        if unit != at(to, to_start.saturating_add(i)) {
            break;
        }
        if unit == Some(BACKSLASH) {
            last_sep = Some(i);
        }
        i = i.saturating_add(1);
    }
    if i == length {
        if to_len > length {
            if at(to, to_start.saturating_add(i)) == Some(BACKSLASH) {
                // `from` is the exact base of `to`.
                return slice(to_start.saturating_add(i).saturating_add(1), to_orig.len());
            }
            if i == 2 {
                // `from` is the device root.
                return slice(to_start.saturating_add(i), to_orig.len());
            }
        }
        if from_len > length {
            if at(from, from_start.saturating_add(i)) == Some(BACKSLASH) {
                last_sep = Some(i);
            } else if i == 2 {
                last_sep = Some(3);
            }
        }
        last_sep = last_sep.or(Some(0));
    }
    let Some(last_sep) = last_sep else {
        // A mismatch before the first common separator: different roots.
        return slice(0, to_orig.len());
    };
    let ups = (from_start.saturating_add(last_sep).saturating_add(1)..=from_end)
        .filter(|&k| k == from_end || at(from, k) == Some(BACKSLASH))
        .count();
    to_start = to_start.saturating_add(last_sep);
    if ups > 0 {
        return [vec![".."; ups].join("\\"), slice(to_start, to_end)].concat();
    }
    if at(to_orig, to_start) == Some(BACKSLASH) {
        to_start = to_start.saturating_add(1);
    }
    slice(to_start, to_end)
}

/// The bounds of `path` without its leading backslashes and, past them, its trailing ones.
fn trimmed(path: &[u16]) -> (usize, usize) {
    let start = path.iter().take_while(|&&u| u == BACKSLASH).count();
    let mut end = path.len();
    while end > start.saturating_add(1) && path.get(end.saturating_sub(1)) == Some(&BACKSLASH) {
        end = end.saturating_sub(1);
    }
    (start, end)
}
