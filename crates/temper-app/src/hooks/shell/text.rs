//! A word's text as the walker builds it: the pieces its parts show, shared with the values
//! they expand, and joined the first time a rule reads the whole (#202 F2).
//!
//! V8 joins strings the same way, lazily, so 1.x answers `: $X/a; ` repeated with a long `$X`
//! at once. A word whose text no rule reads never copies its value; one with a single piece
//! is that piece.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::fmt;
use std::rc::Rc;

use super::words::js_len;

/// The text: its pieces, and the whole once it is read.
#[derive(Clone)]
pub(super) struct Text {
    /// The pieces, each a shared handle; empty once the whole is known from the start.
    pieces: Rc<[Rc<str>]>,
    joined: OnceCell<Rc<str>>,
    /// The whole's length in bytes, known without joining it.
    len: usize,
    /// The whole's length in UTF-16 units, known without joining it.
    units: usize,
}

impl Text {
    /// A text known whole.
    pub(super) fn whole(text: Rc<str>) -> Self {
        let units = js_len(&text);
        Self::whole_of(text, units)
    }

    fn whole_of(text: Rc<str>, units: usize) -> Self {
        Self {
            pieces: Rc::from([]),
            len: text.len(),
            units,
            joined: OnceCell::from(text),
        }
    }

    /// The text the pieces make, in order, `units` UTF-16 units long. An empty piece adds
    /// nothing, so it is not kept.
    pub(super) fn of(mut pieces: Vec<Rc<str>>, units: usize) -> Self {
        pieces.retain(|piece| !piece.is_empty());
        if pieces.len() > 1 {
            return Self {
                len: pieces.iter().map(|p| p.len()).sum(),
                units,
                pieces: Rc::from(pieces),
                joined: OnceCell::new(),
            };
        }
        Self::whole_of(pieces.pop().unwrap_or_else(|| Rc::from("")), units)
    }

    /// The whole's length in bytes.
    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// The whole's length in UTF-16 units.
    pub(super) fn units(&self) -> usize {
        self.units
    }

    /// Whether the whole is `s`, read without joining a text that cannot be.
    pub(super) fn is(&self, s: &str) -> bool {
        self.len == s.len() && self.get() == s
    }

    /// The whole text, joined the first time it is asked for and kept: for the readers that
    /// read a word many times, or read few words. A rule that reads each word of a line once
    /// uses [`Self::read`].
    pub(super) fn get(&self) -> &str {
        self.shared_ref()
    }

    /// Whether reading the whole copies the pieces: there are several, not yet joined.
    pub(super) fn copies(&self) -> bool {
        !self.pieces.is_empty() && self.joined.get().is_none()
    }

    /// The whole text, joined for this read alone when it has not been joined, so the word
    /// keeps no copy (#202 re-review M2).
    pub(super) fn read(&self) -> Cow<'_, str> {
        if let Some(whole) = self.joined.get() {
            return Cow::Borrowed(whole);
        }
        let mut whole = String::with_capacity(self.len);
        for piece in self.pieces.iter() {
            whole.push_str(piece);
        }
        Cow::Owned(whole)
    }

    /// The text after the last `/` or `\`, read from the end without joining the pieces
    /// before it.
    pub(super) fn last_segment(&self) -> Cow<'_, str> {
        if let Some(whole) = self.joined.get() {
            return Cow::Borrowed(after_last_sep(whole));
        }
        let mut tail: Vec<&str> = Vec::new();
        for piece in self.pieces.iter().rev() {
            let after = after_last_sep(piece);
            tail.push(after);
            if after.len() < piece.len() {
                break;
            }
        }
        match tail.as_slice() {
            [one] => Cow::Borrowed(one),
            _ => Cow::Owned(tail.iter().rev().copied().collect()),
        }
    }

    /// At least the first `bytes` bytes of the text (all of it when shorter), to the next
    /// char boundary, without joining the pieces after them.
    pub(super) fn head(&self, bytes: usize) -> Cow<'_, str> {
        let first = self.joined.get().map_or_else(
            || self.pieces.first().map_or("", |p| &**p),
            |whole| &**whole,
        );
        if first.len() >= bytes || self.joined.get().is_some() {
            return Cow::Borrowed(prefix_of(first, bytes));
        }
        let mut head = String::new();
        for piece in self.pieces.iter() {
            let need = bytes.saturating_sub(head.len());
            if need == 0 {
                break;
            }
            head.push_str(prefix_of(piece, need));
        }
        Cow::Owned(head)
    }

    /// The whole text as a shared handle.
    pub(super) fn shared(&self) -> Rc<str> {
        Rc::clone(self.shared_ref())
    }

    fn shared_ref(&self) -> &Rc<str> {
        self.joined.get_or_init(|| {
            let mut whole = String::with_capacity(self.len);
            for piece in self.pieces.iter() {
                whole.push_str(piece);
            }
            Rc::from(whole)
        })
    }
}

/// `s` after its last `/` or `\`, all of it when it has none.
fn after_last_sep(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

/// The first `bytes` bytes of `s`, or more, up to the next char boundary.
fn prefix_of(s: &str, bytes: usize) -> &str {
    let end = (bytes..=s.len())
        .find(|&at| s.is_char_boundary(at))
        .unwrap_or(s.len());
    s.get(..end).unwrap_or(s)
}

#[cfg(test)]
impl Text {
    /// Whether its pieces have been copied into a whole: what reading it costs once.
    pub(super) fn joined(&self) -> bool {
        !self.pieces.is_empty() && self.joined.get().is_some()
    }
}

/// Two texts are equal when their wholes are, however they were built.
impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for Text {}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.get(), f)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::Text;

    /// The pieces are joined once, when the whole is read, and a single piece is shared as it
    /// is; equality reads the wholes.
    #[test]
    fn a_text_is_its_pieces_joined_when_read_and_one_piece_is_shared() {
        let long: Rc<str> = Rc::from("x".repeat(1 << 10));
        let one = Text::of(vec![Rc::clone(&long)], 1 << 10);
        assert!(Rc::ptr_eq(&one.shared(), &long));
        let two = Text::of(vec![Rc::clone(&long), Rc::from("/a")], (1 << 10) + 2);
        assert!(!two.is(";"));
        assert!(two.joined.get().is_none(), "joined before it was read");
        assert_eq!(two.get().len(), (1 << 10) + 2);
        assert!(two.get().ends_with("x/a"));
        assert_eq!(Text::of(Vec::new(), 0).get(), "");
        assert_eq!(
            two,
            Text::whole(Rc::from(["x".repeat(1 << 10), "/a".to_owned()].concat()))
        );
    }

    /// The last segment, the head and a one-off read come from the pieces, and leave the
    /// text unjoined (#202 re-review M2).
    #[test]
    fn a_text_is_read_at_either_end_or_once_without_keeping_a_join() {
        let long: Rc<str> = Rc::from("é".repeat(1 << 10));
        let pieces = |p: &[&Rc<str>]| p.iter().map(|&r| Rc::clone(r)).collect::<Vec<_>>();
        let (slash, name, dir): (Rc<str>, Rc<str>, Rc<str>) =
            (Rc::from("/"), Rc::from("ca"), Rc::from("x/b"));
        let text = Text::of(pieces(&[&slash, &long, &dir, &name]), 0);

        assert_eq!(text.last_segment(), "bca");
        assert_eq!(text.head(2), "/é");
        assert_eq!(text.head(1), "/");
        assert_eq!(text.read().len(), 1 + (2 << 10) + 3 + 2);
        assert!(text.copies(), "a read kept a join");
        let bare = Text::of(pieces(&[&long, &name]), 0);
        assert_eq!(bare.last_segment().len(), (2 << 10) + 2, "no separator: all of it");
        assert_eq!(Text::whole(Rc::from("a\\b/c")).last_segment(), "c");
    }
}
