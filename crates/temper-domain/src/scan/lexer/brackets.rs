//! The depth walkers the rules share (1.x `matchPair`, `bodyBrace`, `matchAngle`, `splitTop` and
//! `statementEnd`): each scans masked code byte by byte, counting brackets as 1.x does.

/// The offset of the `}` closing the `{` at `open`, or the last offset when unbalanced.
pub(crate) fn match_brace(code: &str, open: usize) -> usize {
    match_pair(code, open, b'{', b'}')
}

/// The offset of the `close` byte that closes the `open_byte` at `open`, or the last offset when
/// unbalanced (1.x `matchPair`).
pub(crate) fn match_pair(code: &str, open: usize, open_byte: u8, close: u8) -> usize {
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    let tail = code.as_bytes().iter().enumerate().skip(open);
    for (at, &b) in tail {
        if b == open_byte {
            depth = depth.saturating_add(1);
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return at;
            }
        }
    }
    code.len().saturating_sub(1)
}

/// The offset of the first `{` (a body) at `(`/`[` depth <= 0 from `from`, or `None` when a `;`
/// at that depth (no body) or the end comes first (1.x `bodyBrace`).
pub(crate) fn body_brace(code: &str, from: usize) -> Option<usize> {
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    for (at, &b) in code.as_bytes().iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' => depth = depth.saturating_add(1),
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth <= 0 => return Some(at),
            b';' if depth <= 0 => return None,
            _ => {}
        }
    }
    None
}

/// Is the byte before `at` a `-`, so a `>` at `at` is the arrow of `->`?
fn after_dash(bytes: &[u8], at: usize) -> bool {
    at.checked_sub(1).and_then(|prev| bytes.get(prev)) == Some(&b'-')
}

/// The offset of the `>` closing the generic list whose `<` is at `open` (a `>` after `-` closes
/// none), or `None` when a `{` or `;` comes first or the code ends (1.x `matchAngle`'s -1).
pub(crate) fn match_angle(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    // Signed, as in 1.x: a closer before any opener takes the depth below zero.
    let mut depth = 0_isize;
    for (at, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'<' => depth = depth.saturating_add(1),
            b'>' if !after_dash(bytes, at) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
            b'{' | b';' => return None,
            _ => {}
        }
    }
    None
}

/// `s` split on the ASCII byte `sep` at bracket depth 0, where `<([{` open and `)]}` and a `>`
/// not after `-` close (1.x `splitTop`). Always yields at least one piece: `""` gives `[""]`.
pub(crate) fn split_top(s: &str, sep: u8) -> SplitTop<'_> {
    SplitTop {
        rest: s,
        sep,
        done: false,
    }
}

/// The pieces of [`split_top`], in order.
#[derive(Debug, Clone)]
pub(crate) struct SplitTop<'s> {
    rest: &'s str,
    sep: u8,
    done: bool,
}

impl<'s> Iterator for SplitTop<'s> {
    type Item = &'s str;

    fn next(&mut self) -> Option<&'s str> {
        if self.done {
            return None;
        }
        let bytes = self.rest.as_bytes();
        // Every piece starts at depth 0, since 1.x splits only there.
        let mut depth = 0_isize;
        for (at, &b) in bytes.iter().enumerate() {
            match b {
                b'<' | b'(' | b'[' | b'{' => depth = depth.saturating_add(1),
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b'>' if !after_dash(bytes, at) => depth = depth.saturating_sub(1),
                _ if b == self.sep && depth == 0 => {
                    // `sep` is ASCII, so `at` and `at + 1` are char boundaries.
                    if let Some((piece, tail)) = self.rest.split_at_checked(at) {
                        self.rest = tail.get(1..).unwrap_or("");
                        return Some(piece);
                    }
                }
                _ => {}
            }
        }
        self.done = true;
        Some(self.rest)
    }
}

/// The offset of the `;` that ends the statement or item starting at `from`, or of the closer
/// of the enclosing block when the statement is a tail expression; the length when neither
/// follows (1.x `statementEnd`).
pub(crate) fn statement_end(code: &str, from: usize) -> usize {
    let mut depth = 0_isize;
    for (at, &b) in code.as_bytes().iter().enumerate().skip(from) {
        match b {
            b'(' | b'[' | b'{' => depth = depth.saturating_add(1),
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth < 0 {
                    return at;
                }
            }
            b';' if depth == 0 => return at,
            _ => {}
        }
    }
    code.len()
}
