//! JavaScript semantics the port must reproduce: the `String.prototype.trim` whitespace set,
//! UTF-16 length limits, and the translation of 1.x regex sources to `regex` syntax.

use std::fmt;

/// JS `\s` (the spec's `WhiteSpace` and `LineTerminator`) as the body of a `regex` class.
const JS_SPACE_SET: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";
/// JS `\w` without the `u` flag: ASCII only.
const JS_WORD_SET: &str = "0-9A-Za-z_";
/// JS `.` without the `s` flag: anything but a line terminator.
const JS_DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

/// Is `c` in JS's `\s` set, the set `String.prototype.trim` strips?
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JS `text.trim()`: strips the JS whitespace set, which has U+FEFF and lacks U+0085.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// The longest prefix of `text` that fits in `max_units` UTF-16 code units, cut only between
/// chars, so a surrogate pair is never split (port plan D16(e)).
pub(crate) fn truncate_utf16(text: &str, max_units: usize) -> &str {
    let cut = text
        .char_indices()
        .scan(0_usize, |units, (at, c)| {
            *units = units.saturating_add(c.len_utf16());
            Some((at, *units))
        })
        .find(|&(_, units)| units > max_units)
        .map_or(text.len(), |(at, _)| at);
    // `cut` comes from `char_indices` or is the length, so it is a char boundary.
    text.get(..cut).unwrap_or(text)
}

/// The JS regex flags 1.x patterns use besides `g` (a closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsFlags {
    /// No flag.
    Plain,
}

/// A JS regex construct with no faithful `regex` translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Construct {
    /// `[]`: JS never matches; `regex` would read the `]` as a literal.
    EmptyClass,
    /// `[^]`: JS matches any char, newlines included.
    AnyCharClass,
    /// `--` inside a class: `regex` reads a class difference, JS a range or literals.
    ClassDoubleDash,
    /// A letter or digit escape outside JS's set (`\A`, `\z`, `\p`, `\k`, `\0`, ..), which JS reads
    /// as an identity escape or a back-reference.
    Escape(char),
    /// `\u` without 4 hex digits or `\x` without 2: JS reads a literal `u` or `x`.
    BareHexEscape(char),
}

impl fmt::Display for Construct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyClass => f.write_str("`[]`"),
            Self::AnyCharClass => f.write_str("`[^]`"),
            Self::ClassDoubleDash => f.write_str("`--` in a class"),
            Self::Escape(c) => write!(f, "`\\{c}`"),
            Self::BareHexEscape(c) => write!(f, "`\\{c}` without its hex digits"),
        }
    }
}

/// A 1.x pattern uses a construct [`translate`] cannot reproduce in `regex` syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("JS regex construct {construct} at byte {at} has no faithful translation")]
pub(crate) struct UnsupportedJs {
    pub(crate) construct: Construct,
    /// The byte offset of the construct's first char in the JS source.
    pub(crate) at: usize,
}

/// Translates a 1.x JS regex source to `regex` syntax with the same meaning.
///
/// `\s`, `\w`, `\d` and their negations become explicit classes, `\b` becomes the ASCII word
/// boundary, `.` excludes JS line terminators, and inside a class `\b` is a backspace, `[`, `&`
/// and `~` are literals, and a `-` next to a class escape is a literal. A non-ASCII identity
/// escape becomes the char.
///
/// # Errors
///
/// [`UnsupportedJs`] for a construct that `regex` would accept with another meaning, or that
/// JS reads in a way this translation does not reproduce (see [`Construct`]). Anything else
/// that `regex` cannot read fails when it is compiled, so a bad pattern fails `Scanner::new`,
/// never a scan.
pub(crate) fn translate(js_source: &str, flags: JsFlags) -> Result<String, UnsupportedJs> {
    let mut t = Translator {
        src: js_source,
        flags,
        out: String::with_capacity(js_source.len().saturating_mul(2)),
        class: None,
    };
    let mut at = 0;
    while let Some(c) = js_source.get(at..).and_then(|rest| rest.chars().next()) {
        at = match t.class {
            Some(prev) => t.class_char(c, at, prev)?,
            None => t.char(c, at)?,
        };
    }
    Ok(t.out)
}

/// What came last inside a class, which decides how a `-` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassPrev {
    /// Nothing yet (after `[` or `[^`).
    Start,
    /// An unescaped `-` at this byte offset.
    Dash(usize),
    /// A class escape (`\s`, `\w`, `\d` and their negations).
    ClassEscape,
    /// Any other atom.
    Atom,
}

/// The state of one [`translate`] walk. Each step takes the char at a byte offset and returns
/// the offset after what it consumed.
#[derive(Debug)]
struct Translator<'j> {
    src: &'j str,
    flags: JsFlags,
    out: String,
    /// `None` outside a class.
    class: Option<ClassPrev>,
}

impl<'j> Translator<'j> {
    /// The source after byte `at`.
    fn rest(&self, at: usize) -> &'j str {
        self.src.get(at..).unwrap_or("")
    }

    /// One char outside a class.
    fn char(&mut self, c: char, at: usize) -> Result<usize, UnsupportedJs> {
        let next = at.saturating_add(c.len_utf8());
        match c {
            '\\' => return self.escape(at),
            '[' => return self.open_class(at),
            '.' => self.out.push_str(JS_DOT),
            _ => self.out.push(c),
        }
        Ok(next)
    }

    /// `[` or `[^`, rejecting the JS-only `[]` and `[^]`.
    fn open_class(&mut self, at: usize) -> Result<usize, UnsupportedJs> {
        let body = self.rest(at.saturating_add(1));
        let construct = if body.starts_with(']') {
            Some(Construct::EmptyClass)
        } else if body.starts_with("^]") {
            Some(Construct::AnyCharClass)
        } else {
            None
        };
        if let Some(construct) = construct {
            return Err(UnsupportedJs { construct, at });
        }
        let negated = body.starts_with('^');
        self.out.push_str(if negated { "[^" } else { "[" });
        self.class = Some(ClassPrev::Start);
        Ok(at.saturating_add(if negated { 2 } else { 1 }))
    }

    /// One char inside a class; `prev` is what came before it.
    fn class_char(&mut self, c: char, at: usize, prev: ClassPrev) -> Result<usize, UnsupportedJs> {
        let next = at.saturating_add(c.len_utf8());
        let now = match c {
            '\\' => return self.escape(at),
            ']' => {
                self.out.push(']');
                self.class = None;
                return Ok(next);
            }
            '-' => return self.class_dash(at, prev),
            '[' | '&' | '~' => {
                self.out.push('\\');
                self.out.push(c);
                ClassPrev::Atom
            }
            _ => {
                self.out.push(c);
                ClassPrev::Atom
            }
        };
        self.class = Some(now);
        Ok(next)
    }

    /// A `-` inside a class. Next to a class escape JS reads it as a literal, where `regex` would
    /// read a range or fail, so it becomes `\-`.
    fn class_dash(&mut self, at: usize, prev: ClassPrev) -> Result<usize, UnsupportedJs> {
        if let ClassPrev::Dash(first) = prev {
            return Err(UnsupportedJs {
                construct: Construct::ClassDoubleDash,
                at: first,
            });
        }
        let after = self.rest(at.saturating_add(1));
        let before_escape = after
            .strip_prefix('\\')
            .and_then(|e| e.chars().next())
            .is_some_and(is_class_escape);
        let literal = !after.starts_with(']') && (prev == ClassPrev::ClassEscape || before_escape);
        self.out.push_str(if literal { r"\-" } else { "-" });
        self.class = Some(ClassPrev::Dash(at));
        Ok(at.saturating_add(1))
    }

    /// The escape whose `\` is at byte `at`.
    fn escape(&mut self, at: usize) -> Result<usize, UnsupportedJs> {
        let after = at.saturating_add(1);
        let Some(escaped) = self.rest(after).chars().next() else {
            // A trailing `\` is a JS syntax error; `regex` rejects it too.
            self.out.push('\\');
            return Ok(after);
        };
        let next = after.saturating_add(escaped.len_utf8());
        let in_class = self.class.is_some();
        if is_class_escape(escaped) {
            self.push_class_escape(escaped, in_class);
            if in_class {
                self.class = Some(ClassPrev::ClassEscape);
            }
            return Ok(next);
        }
        let end = match escaped {
            'u' | 'x' => self.hex_escape(at, escaped)?,
            'b' if in_class => {
                self.out.push_str(r"\x08");
                next
            }
            'b' => {
                self.out.push_str(r"(?-u:\b)");
                next
            }
            'B' if !in_class => {
                self.out.push_str(r"(?-u:\B)");
                next
            }
            'f' | 'n' | 'r' | 't' | 'v' => {
                self.out.push('\\');
                self.out.push(escaped);
                next
            }
            c if c.is_ascii_alphanumeric() => {
                return Err(UnsupportedJs {
                    construct: Construct::Escape(c),
                    at,
                });
            }
            // `regex` reads `\<` and `\>` as word-start and word-end assertions, and rejects a
            // non-ASCII escape; JS reads both as the char itself.
            c if c == '<' || c == '>' || !c.is_ascii() => {
                self.out.push(c);
                next
            }
            c => {
                self.out.push('\\');
                self.out.push(c);
                next
            }
        };
        if in_class {
            self.class = Some(ClassPrev::Atom);
        }
        Ok(end)
    }

    /// `\uHHHH` or `\xHH`, copied with its digits; without them JS reads a literal letter.
    fn hex_escape(&mut self, at: usize, letter: char) -> Result<usize, UnsupportedJs> {
        let digits = if letter == 'u' { 4 } else { 2 };
        let body = self.rest(at.saturating_add(2));
        let hex = body
            .get(..digits)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
        let Some(hex) = hex else {
            return Err(UnsupportedJs {
                construct: Construct::BareHexEscape(letter),
                at,
            });
        };
        self.out.push('\\');
        self.out.push(letter);
        self.out.push_str(hex);
        Ok(at.saturating_add(2).saturating_add(digits))
    }

    /// `\s`, `\w`, `\d` or a negation: the set inside a class, a class of its own outside.
    fn push_class_escape(&mut self, escaped: char, in_class: bool) {
        let set = match escaped.to_ascii_lowercase() {
            's' => JS_SPACE_SET,
            'w' => JS_WORD_SET,
            _ => "0-9",
        };
        let negated = escaped.is_ascii_uppercase();
        if in_class && !negated {
            self.out.push_str(set);
        } else {
            self.out.push_str(if negated { "[^" } else { "[" });
            self.out.push_str(set);
            self.out.push(']');
        }
    }
}

/// Is `\c` a JS class escape (`\s`, `\w`, `\d` or a negation)?
fn is_class_escape(c: char) -> bool {
    matches!(c, 's' | 'S' | 'w' | 'W' | 'd' | 'D')
}

#[cfg(test)]
mod tests;
