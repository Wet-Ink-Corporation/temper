//! Directories as the walker carries them: a parent handle plus the names a `cd` added, so a
//! line of `pushd a;` or `cd $d;` holds each directory once, not once per command that ran in
//! it (port plan item 50).
//!
//! 1.x holds every directory as a whole string: `pushd a; ` x 2^15 keeps every directory on
//! the stack, 1.5 GB, and `cd $d; : ; ` keeps one whole copy per command. Here a [`Dir`] is a
//! shared node: the directory it extends and the text it adds. A `cd` costs the value it
//! reads, a `..` the name it drops, and the whole path is built only where a rule reads it
//! ([`Dir::text`]), which [`Guard`] holds to a budget per command line.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use temper_domain::PathStyle;

use super::words::{JS_MAX_STRING, js_len};
use crate::paths;

/// How many bytes of directory text one rule may build from a command line's directories to
/// resolve its paths. Past it the rule stops as it does at V8's longest string, and gives its
/// fallback. A path cannot exist past 4,096 bytes, so only a line that builds directories
/// thousands of times that deep reaches it, and such a line (`cd $d; echo > x; ` x 2000 with
/// an 8,000-byte `$d`) takes 1.x past the hook's 15 s timeout, which answers nothing either
/// (port plan item 50).
const DIR_TEXT_BUDGET: usize = 256 << 20;

/// A directory: absolute, and in Node's normal form when [`Dir::is_normal`].
#[derive(Clone)]
pub(crate) struct Dir(Rc<Node>);

struct Node {
    /// The directory this one extends; `None` for a root node, whose text is the whole path.
    parent: Option<Dir>,
    /// The text this node adds (with its leading separator unless the parent ends in one).
    chunk: Rc<str>,
    /// How much of `chunk` is this node's: `..` cuts a node without copying its text.
    end: usize,
    /// The whole path's length in bytes and in UTF-16 units.
    len: usize,
    units: usize,
    /// It is in Node's normal form, so a relative `cd` may step from it.
    normal: bool,
}

impl Dir {
    /// A path `path.resolve` made, which is in normal form.
    pub(super) fn resolved(path: impl Into<Rc<str>>) -> Self {
        Self::root(path.into(), true)
    }

    /// A path as given (the home directory): a `cd` from it resolves the whole path.
    pub(super) fn given(path: &str) -> Self {
        Self::root(Rc::from(path), false)
    }

    fn root(chunk: Rc<str>, normal: bool) -> Self {
        let end = chunk.len();
        let units = js_len(&chunk);
        Self(Rc::new(Node {
            parent: None,
            chunk,
            end,
            len: end,
            units,
            normal,
        }))
    }

    /// The path's length in UTF-16 units, as JS counts it.
    pub(super) fn units(&self) -> usize {
        self.0.units
    }

    /// Whether a relative `cd` may step from it ([`Dir::step`]).
    pub(super) fn is_normal(&self) -> bool {
        self.0.normal
    }

    /// The whole path. A root is borrowed; a deeper directory is built from its nodes.
    pub(crate) fn text(&self) -> Cow<'_, str> {
        let node = &*self.0;
        let own = node.chunk.get(..node.end).unwrap_or("");
        if node.parent.is_none() {
            return Cow::Borrowed(own);
        }
        let mut pieces = Vec::new();
        let mut at = Some(self);
        while let Some(dir) = at {
            pieces.push(dir.own());
            at = dir.0.parent.as_ref();
        }
        let mut text = String::with_capacity(node.len);
        for piece in pieces.iter().rev() {
            text.push_str(piece);
        }
        Cow::Owned(text)
    }

    fn own(&self) -> &str {
        self.0.chunk.get(..self.0.end).unwrap_or("")
    }

    /// Whether the whole path ends in a separator: only a root can (`/`, `C:\`).
    fn ends_in_sep(&self, sep: char) -> bool {
        self.0.parent.is_none() && self.own().ends_with(sep)
    }

    /// `path.dirname`: the directory one name up; a root's parent is itself.
    fn up(&self, style: PathStyle, sep: char) -> Self {
        let node = &*self.0;
        let own = self.own();
        let Some(parent) = &node.parent else {
            let keep = paths::dirname(style, own).len();
            return self.cut(keep, None);
        };
        match own.rfind(sep) {
            None | Some(0) => parent.clone(), // a shared handle on the parent
            Some(at) => self.cut(at, Some(parent)),
        }
    }

    /// This node with its text cut at `keep` bytes.
    fn cut(&self, keep: usize, parent: Option<&Self>) -> Self {
        let node = &*self.0;
        if keep >= node.end {
            return self.clone(); // a shared handle: nothing to cut
        }
        let dropped = node.chunk.get(keep..node.end).unwrap_or("");
        Self(Rc::new(Node {
            parent: parent.cloned(), // a shared handle on the parent
            chunk: Rc::clone(&node.chunk),
            end: keep,
            len: node.len.saturating_sub(dropped.len()),
            units: node.units.saturating_sub(js_len(dropped)),
            normal: node.normal,
        }))
    }

    /// A node adding `chunk` (which starts with a separator unless `self` ends in one).
    fn child(&self, chunk: String) -> Self {
        let end = chunk.len();
        let units = self.0.units.saturating_add(js_len(&chunk));
        Self(Rc::new(Node {
            parent: Some(self.clone()), // a shared handle on the parent
            chunk: Rc::from(chunk),
            end,
            len: self.0.len.saturating_add(end),
            units,
            normal: self.0.normal,
        }))
    }

    /// `path.resolve(self, value)` for a relative `value`, as Node's normalisation reads it:
    /// each name is appended and each `..` takes the parent, skipping empty names and `.`.
    /// That costs the value, never the directory. `None` when `self` is not in normal form,
    /// or for an empty or absolute value, or on Windows one that names a drive (`D:x`) or
    /// starts at the root (`\x`), for the full resolve to read.
    pub(super) fn step(&self, style: PathStyle, value: &str) -> Option<Self> {
        let windows = style == PathStyle::Windows;
        let rooted = windows && (value.starts_with('\\') || value.contains(':'));
        if !self.is_normal() || value.is_empty() || value.starts_with('/') || rooted {
            return None;
        }
        let sep = if windows { '\\' } else { '/' };
        // Windows reads `\` as a separator too: as `/`, one `memchr` split finds both.
        let value = if windows && value.contains('\\') {
            Cow::Owned(value.replace('\\', "/"))
        } else {
            Cow::Borrowed(value)
        };
        let names = value.split('/');
        let mut base = self.clone(); // a shared handle
        let mut added = String::new();
        for name in names.filter(|name| !name.is_empty() && *name != ".") {
            if name == ".." {
                match added.rfind(sep) {
                    Some(at) => added.truncate(at),
                    None if !added.is_empty() => added.clear(),
                    None => base = base.up(style, sep),
                }
            } else {
                if !(added.is_empty() && base.ends_in_sep(sep)) {
                    added.push(sep);
                }
                added.push_str(name);
            }
        }
        Some(if added.is_empty() {
            base
        } else {
            base.child(added)
        })
    }
}

/// A chain of 2^15 directories would drop each parent from inside its child's drop, one
/// stack frame each: this unlinks the parents one at a time instead, stopping at one that
/// another directory still holds.
impl Drop for Node {
    fn drop(&mut self) {
        let mut parent = self.parent.take();
        while let Some(Dir(node)) = parent {
            parent = match Rc::try_unwrap(node) {
                Ok(mut last) => last.parent.take(),
                Err(_) => None, // still shared: its other holder drops it
            };
        }
    }
}

#[cfg(test)]
impl Dir {
    /// The bytes of text the given directories hold between them, each node counted once:
    /// what a line of them keeps in memory.
    pub(super) fn held_bytes<'d>(dirs: impl IntoIterator<Item = &'d Self>) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut chunks = std::collections::HashSet::new();
        let mut bytes = 0_usize;
        for dir in dirs {
            let mut at = Some(dir);
            while let Some(d) = at {
                if !seen.insert(Rc::as_ptr(&d.0)) {
                    break;
                }
                if chunks.insert(Rc::as_ptr(&d.0.chunk).cast::<u8>()) {
                    bytes = bytes.saturating_add(d.0.chunk.len());
                }
                at = d.0.parent.as_ref();
            }
        }
        bytes
    }

    /// The budget, for its test.
    pub(super) const BUDGET: usize = DIR_TEXT_BUDGET;
}

impl PartialEq for Dir {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.text() == other.text()
    }
}

impl Eq for Dir {}

impl fmt::Debug for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.text(), f)
    }
}

/// What stops the reader and the rules that read its paths, for one command line: a string
/// past V8's longest, where 1.x throws and its rule falls back, or more directory text than
/// [`DIR_TEXT_BUDGET`].
#[derive(Debug, Default)]
pub(crate) struct Guard {
    overflow: Cell<bool>,
    spent: Cell<usize>,
}

impl Guard {
    /// Whether the line went past a bound; the rule then gives its fallback, as 1.x's
    /// `readerRule` does when the reader throws.
    pub(crate) fn overflowed(&self) -> bool {
        self.overflow.get()
    }

    /// Marks the overflow.
    pub(super) fn stop(&self) {
        self.overflow.set(true);
    }

    /// Whether `units` UTF-16 units fit in a JS string; when not, marks the overflow.
    pub(super) fn fits(&self, units: usize) -> bool {
        let fits = units <= JS_MAX_STRING;
        if !fits {
            self.stop();
        }
        fits
    }

    /// `dir`'s whole path, for a rule or a resolve to read, charged to the line's budget;
    /// `None` once the budget is spent, which marks the overflow.
    pub(crate) fn dir_text<'d>(&self, dir: &'d Dir) -> Option<Cow<'d, str>> {
        self.charge(dir).then(|| dir.text())
    }

    /// Charges a copy of `dir`'s whole path to the line's budget; `false` once it is spent,
    /// which marks the overflow.
    pub(super) fn charge(&self, dir: &Dir) -> bool {
        let spent = self.spent.get().saturating_add(dir.0.len);
        self.spent.set(spent);
        if spent > DIR_TEXT_BUDGET {
            self.stop();
            return false;
        }
        true
    }
}
