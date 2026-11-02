//! A word's text as the walker builds it: the pieces its parts show, shared with the values
//! they expand, and joined the first time a rule reads the whole (#202 F2).
//!
//! V8 joins strings the same way, lazily, so 1.x answers `: $X/a; ` repeated with a long `$X`
//! at once. A word whose text no rule reads never copies its value; one with a single piece
//! is that piece.

use std::cell::OnceCell;
use std::fmt;
use std::rc::Rc;

/// The text: its pieces, and the whole once it is read.
#[derive(Clone)]
pub(super) struct Text {
    /// The pieces, each a shared handle; empty once the whole is known from the start.
    pieces: Rc<[Rc<str>]>,
    joined: OnceCell<Rc<str>>,
    /// The whole's length in bytes, known without joining it.
    len: usize,
}

impl Text {
    /// A text known whole.
    pub(super) fn whole(text: Rc<str>) -> Self {
        Self {
            pieces: Rc::from([]),
            len: text.len(),
            joined: OnceCell::from(text),
        }
    }

    /// The text the pieces make, in order. An empty piece adds nothing, so it is not kept.
    pub(super) fn of(mut pieces: Vec<Rc<str>>) -> Self {
        pieces.retain(|piece| !piece.is_empty());
        if pieces.len() > 1 {
            return Self {
                len: pieces.iter().map(|p| p.len()).sum(),
                pieces: Rc::from(pieces),
                joined: OnceCell::new(),
            };
        }
        Self::whole(pieces.pop().unwrap_or_else(|| Rc::from("")))
    }

    /// Whether the whole is `s`, read without joining a text that cannot be.
    pub(super) fn is(&self, s: &str) -> bool {
        self.len == s.len() && self.get() == s
    }

    /// The whole text, joined the first time it is asked for.
    pub(super) fn get(&self) -> &str {
        self.shared_ref()
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
        let one = Text::of(vec![Rc::clone(&long)]);
        assert!(Rc::ptr_eq(&one.shared(), &long));
        let two = Text::of(vec![Rc::clone(&long), Rc::from("/a")]);
        assert!(!two.is(";"));
        assert!(two.joined.get().is_none(), "joined before it was read");
        assert_eq!(two.get().len(), (1 << 10) + 2);
        assert!(two.get().ends_with("x/a"));
        assert_eq!(Text::of(Vec::new()).get(), "");
        assert_eq!(
            two,
            Text::whole(Rc::from(["x".repeat(1 << 10), "/a".to_owned()].concat()))
        );
    }
}
