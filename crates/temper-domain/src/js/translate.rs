//! The translation of 1.x JS regex sources, with their flags, to `regex` syntax.

use std::fmt;

use super::{JS_DOT, JS_SPACE_SET, JS_WORD_SET};

/// The JS regex flags 1.x patterns use besides `g` (a closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsFlags {
    /// No flag.
    Plain,
    /// `m`: `^` also matches after each JS line terminator (`\n`, `\r`, U+2028, U+2029).
    Multiline,
    /// `i` without `u`: ASCII letters match either case. JS never folds an ASCII letter with a
    /// non-ASCII char (`ſ`, the Kelvin sign), which `regex`'s `(?i)` would.
    IgnoreCase,
}

/// JS line terminators other than `\n`, as a `regex` class; the `m` translation of `^` consumes
/// one of them, since `regex`'s multi-line `^` follows only `\n`.
const JS_LINE_START: &str = r"(?:(?m:^)|[\r\x{2028}\x{2029}])";

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
    /// `$` under `m`: JS also ends a line before `\r`, U+2028 and U+2029.
    MultilineDollar,
    /// Under `i`, what JS folds beyond single ASCII letters: a non-ASCII char, a `\u` or `\x`
    /// escape, or a class range.
    Folded,
}

impl fmt::Display for Construct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyClass => f.write_str("`[]`"),
            Self::AnyCharClass => f.write_str("`[^]`"),
            Self::ClassDoubleDash => f.write_str("`--` in a class"),
            Self::Escape(c) => write!(f, "`\\{c}`"),
            Self::BareHexEscape(c) => write!(f, "`\\{c}` without its hex digits"),
            Self::MultilineDollar => f.write_str("`$` under the m flag"),
            Self::Folded => f.write_str("a char or range the i flag folds"),
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
        let multiline = self.flags == JsFlags::Multiline;
        match c {
            '\\' => return self.escape(at),
            '[' => return self.open_class(at),
            '.' => self.out.push_str(JS_DOT),
            '^' if multiline => self.out.push_str(JS_LINE_START),
            '$' if multiline => {
                return Err(UnsupportedJs {
                    construct: Construct::MultilineDollar,
                    at,
                });
            }
            _ => self.literal(c, at)?,
        }
        Ok(next)
    }

    /// The literal char `c` at byte `at`. Under `i`, an ASCII letter gets its other case: as a
    /// class of two outside a class, beside it inside one.
    fn literal(&mut self, c: char, at: usize) -> Result<(), UnsupportedJs> {
        if self.flags != JsFlags::IgnoreCase || (c.is_ascii() && !c.is_ascii_alphabetic()) {
            self.out.push(c);
            return Ok(());
        }
        if !c.is_ascii() {
            return Err(UnsupportedJs {
                construct: Construct::Folded,
                at,
            });
        }
        let pair = [c.to_ascii_lowercase(), c.to_ascii_uppercase()];
        if self.class.is_some() {
            self.out.extend(pair);
        } else {
            self.out.push('[');
            self.out.extend(pair);
            self.out.push(']');
        }
        Ok(())
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
                self.literal(c, at)?;
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
        let last = after.starts_with(']');
        let literal = !last && (prev == ClassPrev::ClassEscape || before_escape);
        let range = !last && !literal && prev != ClassPrev::Start;
        if range && self.flags == JsFlags::IgnoreCase {
            return Err(UnsupportedJs {
                construct: Construct::Folded,
                at,
            });
        }
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
        let end = if matches!(escaped, 'u' | 'x') {
            self.hex_escape(at, escaped)?
        } else {
            self.other_escape(escaped, at, in_class)?;
            next
        };
        if in_class {
            self.class = Some(ClassPrev::Atom);
        }
        Ok(end)
    }

    /// An escape other than a class or hex escape: `\escaped`, whose `\` is at byte `at`.
    fn other_escape(
        &mut self,
        escaped: char,
        at: usize,
        in_class: bool,
    ) -> Result<(), UnsupportedJs> {
        match escaped {
            'b' if in_class => self.out.push_str(r"\x08"),
            'b' => self.out.push_str(r"(?-u:\b)"),
            'B' if !in_class => self.out.push_str(r"(?-u:\B)"),
            'f' | 'n' | 'r' | 't' | 'v' => {
                self.out.push('\\');
                self.out.push(escaped);
            }
            c if c.is_ascii_alphanumeric() => {
                return Err(UnsupportedJs {
                    construct: Construct::Escape(c),
                    at,
                });
            }
            // `regex` reads `\<` and `\>` as word-start and word-end assertions; JS as literals.
            '<' | '>' => self.out.push(escaped),
            // `regex` rejects a non-ASCII escape; JS reads it as the char itself.
            c if !c.is_ascii() => self.literal(c, at)?,
            c => {
                self.out.push('\\');
                self.out.push(c);
            }
        }
        Ok(())
    }

    /// `\uHHHH` or `\xHH`, copied with its digits; without them JS reads a literal letter.
    fn hex_escape(&mut self, at: usize, letter: char) -> Result<usize, UnsupportedJs> {
        if self.flags == JsFlags::IgnoreCase {
            // The escaped char may be a letter, which JS would fold.
            return Err(UnsupportedJs {
                construct: Construct::Folded,
                at,
            });
        }
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
