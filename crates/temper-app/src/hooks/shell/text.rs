//! A word's text as the walker builds it: the pieces its parts show, shared with the values
//! they expand, and joined the first time a rule reads the whole (#202 F2).
//!
//! V8 joins strings the same way, lazily, so 1.x answers `: $X/a; ` repeated with a long `$X`
//! at once. A word whose text no rule reads never copies its value; one with a single piece
//! is that piece. Every join is charged to the line's [`Guard`], a kept one once, so what a
//! line joins, and keeps, is held to its budget (#202 re-review M2).

use std::borrow::Cow;
use std::cell::OnceCell;
use std::fmt;
use std::rc::Rc;

use super::dir::Guard;
use super::words::js_len;

/// The text: its pieces, and the whole once it is read.
#[derive(Clone)]
pub(super) struct Text {
    /// The pieces, each a shared handle; empty once the whole is known from the start.
    pieces: Rc<[Rc<str>]>,
    joined: OnceCell<Rc<str>>,
    /// The line's budget, which each join of the pieces is charged to; `None` for a text
    /// known whole, or one made outside a walk.
    meter: Option<Guard>,
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
            meter: None,
        }
    }

    /// The text the pieces make, in order, `units` UTF-16 units long, whose joins are
    /// charged to `meter`. An empty piece adds nothing, so it is not kept.
    pub(super) fn of(mut pieces: Vec<Rc<str>>, units: usize, meter: Option<&Guard>) -> Self {
        pieces.retain(|piece| !piece.is_empty());
        if pieces.len() > 1 {
            return Self {
                len: pieces.iter().map(|p| p.len()).sum(),
                units,
                pieces: Rc::from(pieces),
                joined: OnceCell::new(),
                meter: meter.cloned(), // a shared handle on the line's budget
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
    /// uses [`Self::read`]. Past the line's budget it is empty and the guard is stopped, so
    /// whatever reads the line gives its fallback.
    pub(super) fn get(&self) -> &str {
        self.shared_ref().map_or("", |whole| whole)
    }

    /// Whether reading the whole copies the pieces: there are several, not yet joined.
    #[cfg(test)]
    pub(super) fn copies(&self) -> bool {
        !self.pieces.is_empty() && self.joined.get().is_none()
    }

    /// The whole text, joined for this read alone when it has not been joined, so the word
    /// keeps no copy (#202 re-review M2); `None` once the line's budget is spent, which stops
    /// the guard.
    pub(super) fn read(&self) -> Option<Cow<'_, str>> {
        if let Some(whole) = self.joined.get() {
            return Some(Cow::Borrowed(whole));
        }
        self.charged().then(|| Cow::Owned(self.join()))
    }

    /// The whole text for the walker to store as a variable's value: joined for this read
    /// alone and not charged here, since the variables charge the value they keep, net of
    /// the ones they free ([`super::words::Vars`], #202 third review F2), so `a=$a/b`
    /// repeated holds one value at a time to the budget.
    pub(super) fn unkept(&self) -> Cow<'_, str> {
        match self.joined.get() {
            Some(whole) => Cow::Borrowed(whole),
            None => Cow::Owned(self.join()),
        }
    }

    /// Whether the line's budget has room for one more join of the pieces, charging it.
    fn charged(&self) -> bool {
        self.meter
            .as_ref()
            .is_none_or(|guard| guard.charge(self.len))
    }

    fn join(&self) -> String {
        let mut whole = String::with_capacity(self.len);
        for piece in self.pieces.iter() {
            whole.push_str(piece);
        }
        whole
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

    /// The whole text as a shared handle; empty past the line's budget, as [`Self::get`] is.
    pub(super) fn shared(&self) -> Rc<str> {
        self.shared_ref().map_or_else(|| Rc::from(""), Rc::clone)
    }

    /// The kept join, made and charged the first time; `None` when the line's budget has no
    /// room for it.
    fn shared_ref(&self) -> Option<&Rc<str>> {
        if let Some(whole) = self.joined.get() {
            return Some(whole);
        }
        if !self.charged() {
            return None;
        }
        Some(self.joined.get_or_init(|| Rc::from(self.join())))
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

impl Text {
    /// What the whole is made of, in order: the kept join, or the pieces.
    fn chunks(&self) -> &[Rc<str>] {
        self.joined
            .get()
            .map_or(&self.pieces, |whole| std::slice::from_ref(whole))
    }
}

/// Two texts are equal when their wholes are, however they were built. They are compared
/// piece by piece, so neither keeps a join or charges the line's guard (#202 third review F5).
impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && same_bytes(self.chunks(), other.chunks())
    }
}

impl Eq for Text {}

/// Whether two runs of chunks hold the same bytes, read without joining either.
fn same_bytes(left: &[Rc<str>], right: &[Rc<str>]) -> bool {
    let mut left = left.iter().map(|c| c.as_bytes());
    let mut right = right.iter().map(|c| c.as_bytes());
    let (mut a, mut b): (&[u8], &[u8]) = (&[], &[]);
    loop {
        if a.is_empty() {
            match left.next() {
                Some(next) => a = next,
                None => return b.is_empty() && right.all(<[u8]>::is_empty),
            }
            continue;
        }
        if b.is_empty() {
            match right.next() {
                Some(next) => b = next,
                None => return false,
            }
            continue;
        }
        let n = a.len().min(b.len());
        let (Some((a_head, a_rest)), Some((b_head, b_rest))) =
            (a.split_at_checked(n), b.split_at_checked(n))
        else {
            return false;
        };
        if a_head != b_head {
            return false;
        }
        (a, b) = (a_rest, b_rest);
    }
}

/// Prints the whole, joined for the print alone: neither kept nor charged (#202 third
/// review F5).
impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.unkept(), f)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::Text;
    use crate::hooks::shell::Guard;

    /// The pieces are joined once, when the whole is read, and a single piece is shared as it
    /// is; equality reads the wholes.
    #[test]
    fn a_text_is_its_pieces_joined_when_read_and_one_piece_is_shared() {
        let long: Rc<str> = Rc::from("x".repeat(1 << 10));
        let one = Text::of(vec![Rc::clone(&long)], 1 << 10, None);
        assert!(Rc::ptr_eq(&one.shared(), &long));
        let two = Text::of(vec![Rc::clone(&long), Rc::from("/a")], (1 << 10) + 2, None);
        assert!(!two.is(";"));
        assert!(two.joined.get().is_none(), "joined before it was read");
        assert_eq!(two.get().len(), (1 << 10) + 2);
        assert!(two.get().ends_with("x/a"));
        assert_eq!(Text::of(Vec::new(), 0, None).get(), "");
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
        let text = Text::of(pieces(&[&slash, &long, &dir, &name]), 0, None);

        assert_eq!(text.last_segment(), "bca");
        assert_eq!(text.head(2), "/é");
        assert_eq!(text.head(1), "/");
        assert_eq!(text.read().map(|r| r.len()), Some(1 + (2 << 10) + 3 + 2));
        assert!(text.copies(), "a read kept a join");
        let bare = Text::of(pieces(&[&long, &name]), 0, None);
        assert_eq!(
            bare.last_segment().len(),
            (2 << 10) + 2,
            "no separator: all of it"
        );
        assert_eq!(Text::whole(Rc::from("a\\b/c")).last_segment(), "c");
    }

    /// #202 re-review M2: every join is charged to the line's guard, a kept one once however
    /// often it is read, so the joins a line keeps are held to its budget. Past it the text
    /// reads as empty and the guard is stopped, which makes the reader and the rules fall
    /// back.
    #[test]
    fn a_join_is_charged_to_the_lines_guard_and_past_its_budget_the_line_falls_back() {
        let long: Rc<str> = Rc::from("x".repeat(1 << 10));
        let guard = Guard::with_budget(3 << 10);
        let text = |tail: &str| Text::of(vec![Rc::clone(&long), Rc::from(tail)], 0, Some(&guard));
        let (kept, once, over) = (text("/a"), text("/b"), text("/c"));

        assert_eq!(kept.get().len(), (1 << 10) + 2);
        assert!(
            kept.get().ends_with("x/a"),
            "read again from the kept join, free"
        );
        assert_eq!(once.read().map(|r| r.len()), Some((1 << 10) + 2));
        assert!(!guard.overflowed());
        assert_eq!(over.get(), "", "past the budget");
        assert!(guard.overflowed());
        assert!(!over.joined(), "a join past the budget is not kept");
        assert_eq!(over.read(), None);
        assert_eq!(
            kept.get().len(),
            (1 << 10) + 2,
            "a kept join still reads whole"
        );
    }

    /// #202 third review F5: comparing or printing a text reads its pieces and neither
    /// keeps a join nor charges the line's guard.
    #[test]
    fn comparing_or_printing_a_text_keeps_no_join_and_charges_nothing() {
        let guard = Guard::with_budget(1 << 20);
        let text = |p: &[&str]| {
            let pieces = p.iter().map(|&s| Rc::from(s)).collect();
            Text::of(pieces, 0, Some(&guard))
        };
        let (left, right) = (text(&["ab", "cd", "e"]), text(&["a", "bcde"]));
        assert_eq!(left, right);
        assert_ne!(left, text(&["ab", "cd", "f"]));
        assert_ne!(left, text(&["ab", "cd"]));
        assert_eq!(left, Text::whole(Rc::from("abcde")));
        assert_eq!(format!("{left:?}"), "\"abcde\"");
        assert!(left.copies() && right.copies(), "a join was kept");
        assert_eq!(guard.spent(), 0);
    }
}
