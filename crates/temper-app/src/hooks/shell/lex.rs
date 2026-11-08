//! The lexer (1.x `shell.mjs` `lex`, `balanced`, `expansion`, `ansiC`): a command line as the
//! simple commands it is written as, with quotes, escapes, expansions, heredocs, redirects
//! and operators read the way bash reads them.
//!
//! Every branch tests an ASCII byte, so the lexer walks byte offsets and copies other chars
//! whole. Each scan moves forward only: a balanced `$(`, `${` or `<(` is scanned once at its
//! level, and a heredoc's lines once in all, so the cost is linear in the line at each nesting
//! level the walker reads.

use std::ops::Range;
use std::rc::Rc;

use temper_domain::is_js_space;

/// What one part of a word is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PartKind {
    /// Literal text.
    Lit,
    /// A backslash kept before a letter, digit, `_` or `-` outside quotes (1.x `bs`): bash
    /// drops it and PowerShell reads it as a separator, so the walker decides.
    Escape,
    /// `$NAME`, `${NAME}` or PowerShell's `$env:NAME`, with the name.
    Var(String),
    /// Any other `${...}`, or a `$'...'` with an escape temper does not decode exactly.
    Param,
    /// `$(...)` or backquotes, with the script inside.
    Sub(String),
    /// `$((...))`.
    Arith,
    /// `<(...)` or `>(...)`, with the script inside.
    Proc(String),
}

/// One part of a word: its kind, its text as written (a literal's text with its quotes and
/// escapes removed) and whether it was quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Part {
    pub(super) kind: PartKind,
    pub(super) text: String,
    pub(super) quoted: bool,
}

impl Part {
    /// 1.x `p.t === 'lit'`, which a kept backslash is too.
    pub(super) fn is_lit(&self) -> bool {
        matches!(self.kind, PartKind::Lit | PartKind::Escape)
    }
}

/// A word as written: its parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RawWord {
    pub(super) parts: Vec<Part>,
}

/// A redirection operator (1.x `REDIRECT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RedirectOp {
    /// `&>>`
    BothAppend,
    /// `&>`
    Both,
    /// `>>`
    Append,
    /// `>|`
    Clobber,
    /// `>&` with a file
    DupOut,
    /// `>`
    Out,
    /// `<<<`
    HereString,
    /// `<<-`
    HeredocTabs,
    /// `<<`
    Heredoc,
    /// `<&` with a file
    DupIn,
    /// `<>`
    ReadWrite,
    /// `<`
    In,
}

/// The operators in 1.x's alternation order, longest first where one is a prefix of another.
const REDIRECTS: [(&str, RedirectOp); 12] = [
    ("&>>", RedirectOp::BothAppend),
    ("&>", RedirectOp::Both),
    (">>", RedirectOp::Append),
    (">|", RedirectOp::Clobber),
    (">&", RedirectOp::DupOut),
    (">", RedirectOp::Out),
    ("<<<", RedirectOp::HereString),
    ("<<-", RedirectOp::HeredocTabs),
    ("<<", RedirectOp::Heredoc),
    ("<&", RedirectOp::DupIn),
    ("<>", RedirectOp::ReadWrite),
    ("<", RedirectOp::In),
];

impl RedirectOp {
    /// 1.x `WRITE_REDIRECTS`: the operators that write their target.
    pub(super) fn writes(self) -> bool {
        matches!(
            self,
            Self::Out
                | Self::Append
                | Self::Clobber
                | Self::Both
                | Self::BothAppend
                | Self::DupOut
                | Self::ReadWrite
        )
    }
}

/// A heredoc's delimiter and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Heredoc {
    delim: String,
    /// The delimiter was quoted or escaped, so the body is not expanded.
    pub(super) quoted: bool,
    /// The lines up to the delimiter, each ending in `\n`; `None` when no newline followed
    /// the operator's line.
    pub(super) body: Option<Rc<str>>,
}

/// A redirection with its target word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawRedirect {
    pub(super) op: RedirectOp,
    pub(super) word: RawWord,
    pub(super) heredoc: Option<Heredoc>,
}

/// The operator before or after a simple command, as the walker reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Joint {
    /// `|`
    Pipe,
    /// `|&`
    PipeBoth,
    /// `&`
    Background,
    /// Any other operator, a newline, or none.
    #[default]
    Other,
}

impl Joint {
    fn of(op: &str) -> Self {
        match op {
            "|" => Self::Pipe,
            "|&" => Self::PipeBoth,
            "&" => Self::Background,
            _ => Self::Other,
        }
    }

    /// `|` or `|&`.
    pub(super) fn is_pipe(self) -> bool {
        matches!(self, Self::Pipe | Self::PipeBoth)
    }
}

/// A simple command as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RawCommand {
    pub(super) words: Vec<RawWord>,
    pub(super) redirects: Vec<RawRedirect>,
    /// The operator before it.
    pub(super) sep: Joint,
    /// The operator after it.
    pub(super) after: Joint,
}

/// What the lexer reads: simple commands and a subshell's parentheses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    Command(RawCommand),
    Open,
    Close,
}

/// 1.x `OPERATOR`, in its alternation order.
const OPERATORS: [&str; 11] = [";;&", ";;", ";&", "&&", "||", "|&", ";", "&", "|", "(", ")"];

/// What [`lex`] reads from a source.
pub(super) struct Lexed {
    pub(super) items: Vec<Item>,
    /// 1.x `items.bodies`: where each heredoc body is in the source, in order.
    pub(super) bodies: Vec<Range<usize>>,
}

/// 1.x `lex(src)`.
pub(super) fn lex(src: &str) -> Lexed {
    let mut lexer = Lexer {
        s: src,
        i: 0,
        out: Vec::new(),
        cmd: None,
        word: None,
        redir: None,
        sep: Joint::Other,
        heredocs: Vec::new(),
        bodies: Vec::new(),
    };
    lexer.run();
    Lexed {
        items: lexer.out,
        bodies: lexer.bodies,
    }
}

/// 1.x's `line`: `src` without its heredoc bodies, the code a PowerShell expression is read
/// from.
pub(super) fn without_bodies(src: &str, bodies: &[Range<usize>]) -> String {
    let mut line = String::with_capacity(src.len());
    let mut at = 0;
    for body in bodies {
        line.push_str(src.get(at..body.start).unwrap_or(""));
        at = body.end.max(at);
    }
    line.push_str(src.get(at..).unwrap_or(""));
    line
}

struct Lexer<'s> {
    s: &'s str,
    i: usize,
    out: Vec<Item>,
    cmd: Option<RawCommand>,
    word: Option<RawWord>,
    redir: Option<RedirectOp>,
    sep: Joint,
    /// The heredocs waiting for the next newline: the item and redirect each one is.
    heredocs: Vec<(usize, usize)>,
    /// Where each heredoc body read so far is.
    bodies: Vec<Range<usize>>,
}

impl Lexer<'_> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.s.as_bytes().get(at).copied()
    }

    fn rest(&self, from: usize) -> &str {
        self.s.get(from..).unwrap_or("")
    }

    fn slice(&self, from: usize, to: usize) -> &str {
        self.s.get(from..to.min(self.s.len())).unwrap_or("")
    }

    fn ensure(&mut self) -> &mut RawCommand {
        let sep = self.sep;
        self.cmd.get_or_insert_with(|| RawCommand {
            sep,
            ..RawCommand::default()
        })
    }

    fn start_word(&mut self) -> &mut RawWord {
        self.word.get_or_insert_with(RawWord::default)
    }

    /// 1.x `lit(text, q)`: appends to the last part when it is a literal of the same quoting.
    fn lit(&mut self, text: &str, quoted: bool) {
        let word = self.start_word();
        match word.parts.last_mut() {
            Some(last) if last.kind == PartKind::Lit && last.quoted == quoted => {
                last.text.push_str(text);
            }
            _ => word.parts.push(Part {
                kind: PartKind::Lit,
                text: text.to_owned(),
                quoted,
            }),
        }
    }

    fn end_word(&mut self) {
        let Some(word) = self.word.take() else {
            return;
        };
        let redir = self.redir.take();
        let cmd = self.ensure();
        let Some(op) = redir else {
            cmd.words.push(word);
            return;
        };
        let heredoc = matches!(op, RedirectOp::Heredoc | RedirectOp::HeredocTabs).then(|| {
            // `<<\EOF` is a quoted EOF.
            let delim = word
                .parts
                .iter()
                .filter(|p| p.kind != PartKind::Escape)
                .map(|p| p.text.as_str())
                .collect();
            let quoted = word
                .parts
                .iter()
                .any(|p| p.quoted || p.kind == PartKind::Escape);
            Heredoc {
                delim,
                quoted,
                body: None,
            }
        });
        let waiting = heredoc.is_some();
        cmd.redirects.push(RawRedirect { op, word, heredoc });
        let at = cmd.redirects.len().saturating_sub(1);
        if waiting {
            // The command is pushed at the next operator or newline, before any body is read.
            let item = self.out.len();
            self.heredocs.push((item, at));
        }
    }

    fn end_cmd(&mut self, op: &str) {
        self.end_word();
        self.redir = None;
        if let Some(mut cmd) = self.cmd.take() {
            cmd.after = Joint::of(op);
            self.out.push(Item::Command(cmd));
        }
        self.sep = Joint::of(op);
    }

    fn run(&mut self) {
        while let Some(c) = self.byte(self.i) {
            let next = self.byte(self.i.saturating_add(1));
            match c {
                b'\\' => self.backslash(),
                b'\'' => self.single_quoted(),
                b'"' => self.double_quoted(),
                b'$' if next == Some(b'"') => {
                    // Locale quoting is a double-quoted string to bash (#155).
                    self.i = self.i.saturating_add(1);
                }
                b'$' if next == Some(b'\'') => self.ansi_c_quoted(),
                b'$' | b'`' => self.unquoted_expansion(),
                b'#' if self.word.is_none() => {
                    self.i = self
                        .rest(self.i)
                        .find('\n')
                        .map_or(self.s.len(), |e| self.i.saturating_add(e));
                }
                b'\n' => {
                    self.end_cmd("\n");
                    self.i = self.i.saturating_add(1);
                    self.read_heredocs();
                }
                b' ' | b'\t' | b'\r' => {
                    self.end_word();
                    self.i = self.i.saturating_add(1);
                }
                b'<' | b'>' if next == Some(b'(') => self.process_substitution(),
                _ => {
                    if !self.redirect() && !self.operator() {
                        self.plain();
                    }
                }
            }
        }
        self.end_cmd("");
    }

    /// `'...'`: literal text up to the next quote.
    fn single_quoted(&mut self) {
        let from = self.i.saturating_add(1);
        let end = self
            .rest(from)
            .find('\'')
            .map_or(self.s.len(), |e| from.saturating_add(e));
        let text = self.slice(from, end).to_owned();
        self.lit(&text, true);
        self.i = end.saturating_add(1);
    }

    /// `$'...'`: the decoded text, or a parameter part when temper cannot decode it.
    fn ansi_c_quoted(&mut self) {
        let (text, j) = ansi_c(self.s, self.i.saturating_add(2));
        if let Some(text) = text {
            self.lit(&text, true);
        } else {
            let part = Part {
                kind: PartKind::Param,
                text: self.slice(self.i, j.saturating_add(1)).to_owned(),
                quoted: true,
            };
            self.start_word().parts.push(part);
        }
        self.i = j.saturating_add(1);
    }

    /// `$...` or a backquote outside quotes: the expansion, or a literal `$`.
    fn unquoted_expansion(&mut self) {
        if let Some((part, next)) = expansion(self.s, self.i, false) {
            self.start_word().parts.push(part);
            self.i = next;
        } else {
            self.lit("$", false);
            self.i = self.i.saturating_add(1);
        }
    }

    /// `<(...)` or `>(...)`.
    fn process_substitution(&mut self) {
        let e = balanced(self.s, self.i.saturating_add(1), b'(', b')');
        let part = Part {
            kind: PartKind::Proc(self.slice(self.i.saturating_add(2), e).to_owned()),
            text: self.slice(self.i, e.saturating_add(1)).to_owned(),
            quoted: false,
        };
        self.start_word().parts.push(part);
        self.i = e.saturating_add(1);
    }

    /// One char outside quotes that starts nothing: literal text.
    fn plain(&mut self) {
        let at = self.i;
        let len = self.rest(at).chars().next().map_or(1, char::len_utf8);
        let next = at.saturating_add(len);
        let text = self.slice(at, next).to_owned();
        self.lit(&text, false);
        self.i = next;
    }

    fn backslash(&mut self) {
        let i = self.i;
        let next = self.rest(i.saturating_add(1)).chars().next();
        match next {
            Some('\n') => self.i = i.saturating_add(2),
            Some('\r') if self.byte(i.saturating_add(2)) == Some(b'\n') => {
                self.i = i.saturating_add(3);
            }
            Some(c) if !keeps_backslash(c) => {
                let mut buf = [0; 4];
                self.lit(c.encode_utf8(&mut buf), true);
                self.i = i.saturating_add(1).saturating_add(c.len_utf8());
            }
            Some(_) => {
                self.start_word().parts.push(Part {
                    kind: PartKind::Escape,
                    text: "\\".to_owned(),
                    quoted: false,
                });
                self.i = i.saturating_add(1);
            }
            None => {
                self.lit("\\", false);
                self.i = i.saturating_add(1);
            }
        }
    }

    fn double_quoted(&mut self) {
        let n = self.s.len();
        self.start_word();
        self.lit("", true);
        let mut j = self.i.saturating_add(1);
        while let Some(c) = self.byte(j).filter(|&c| c != b'"') {
            let after = self.byte(j.saturating_add(1));
            if c == b'\\' && after.is_some_and(|a| b"\"\\$`\n".contains(&a)) {
                if after != Some(b'\n') {
                    let text = self
                        .slice(j.saturating_add(1), j.saturating_add(2))
                        .to_owned();
                    self.lit(&text, true);
                }
                j = j.saturating_add(2);
                continue;
            }
            if (c == b'$' || c == b'`')
                && let Some((part, next)) = expansion(self.s, j, true)
            {
                self.start_word().parts.push(part);
                j = next;
                continue;
            }
            // A run of plain chars at once (1.x `DQ_PLAIN`), so a long string stays fast.
            let len = self.rest(j).chars().next().map_or(1, char::len_utf8);
            let from = j.saturating_add(len);
            let run = self
                .rest(from)
                .find(['"', '\\', '$', '`'])
                .unwrap_or_else(|| n.saturating_sub(from));
            let end = from.saturating_add(run);
            let text = self.slice(j, end).to_owned();
            self.lit(&text, true);
            j = end;
        }
        self.i = j.saturating_add(1);
    }

    /// A redirection operator at the cursor, read; false when there is none.
    fn redirect(&mut self) -> bool {
        let rest = self.rest(self.i);
        let Some(&(text, op)) = REDIRECTS.iter().find(|(text, _)| rest.starts_with(text)) else {
            return false;
        };
        // `2>x`: an unquoted number just before the operator is its descriptor, not a word.
        let descriptor = self
            .word
            .as_ref()
            .is_some_and(|w| match w.parts.as_slice() {
                [only] => {
                    only.kind == PartKind::Lit
                        && !only.quoted
                        && !only.text.is_empty()
                        && only.text.bytes().all(|b| b.is_ascii_digit())
                }
                _ => false,
            });
        if descriptor {
            self.word = None;
            self.ensure();
        } else {
            self.end_word();
        }
        self.ensure();
        self.i = self.i.saturating_add(text.len());
        // `2>&1`, `>&-`, `<&3`: a descriptor, not a file.
        if matches!(op, RedirectOp::DupOut | RedirectOp::DupIn)
            && let Some(len) = descriptor_len(self.rest(self.i))
        {
            self.i = self.i.saturating_add(len);
            return true;
        }
        self.redir = Some(op);
        true
    }

    /// A control operator at the cursor, read; false when there is none.
    fn operator(&mut self) -> bool {
        let rest = self.rest(self.i);
        let Some(op) = OPERATORS.iter().find(|op| rest.starts_with(**op)) else {
            return false;
        };
        if *op == "("
            && self.cmd.is_none()
            && self.word.is_none()
            && self.byte(self.i.saturating_add(1)) == Some(b'(')
        {
            // `(( arithmetic ))`.
            self.i = balanced(self.s, self.i, b'(', b')').saturating_add(1);
            return true;
        }
        self.end_cmd(op);
        self.i = self.i.saturating_add(op.len());
        match *op {
            "(" => self.out.push(Item::Open),
            ")" => self.out.push(Item::Close),
            _ => {}
        }
        true
    }

    /// The bodies of the heredocs opened on the line just ended, in order.
    fn read_heredocs(&mut self) {
        let s = self.s;
        for (item, at) in std::mem::take(&mut self.heredocs) {
            let Some(Item::Command(cmd)) = self.out.get_mut(item) else {
                continue;
            };
            let Some(RawRedirect {
                op,
                heredoc: Some(heredoc),
                ..
            }) = cmd.redirects.get_mut(at)
            else {
                continue;
            };
            let tabs = *op == RedirectOp::HeredocTabs;
            let (body, next) = heredoc_body(s, self.i, &heredoc.delim, tabs);
            heredoc.body = Some(body);
            self.bodies.push(self.i..next.min(s.len()));
            self.i = next;
        }
    }
}

/// The lines of `s` from `from` up to the one that is `delim`, which ends the body, and the
/// index after that line.
fn heredoc_body(s: &str, mut i: usize, delim: &str, tabs: bool) -> (Rc<str>, usize) {
    let n = s.len();
    let mut body = String::new();
    while i < n {
        let from = i;
        let rest = s.get(from..).unwrap_or("");
        let end = rest.find('\n').map_or(n, |e| from.saturating_add(e));
        let line = s.get(from..end).unwrap_or("");
        i = end.saturating_add(1);
        let line = if tabs {
            line.trim_start_matches('\t')
        } else {
            line
        };
        if line.strip_suffix('\r').unwrap_or(line) == delim {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    (Rc::from(body), i)
}

/// 1.x `KEEP_BACKSLASH`: `[\w-]`.
pub(super) fn keeps_backslash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// 1.x `/^[ \t]*(?:\d+|-)(?=$|[\s;&|<>()])/`: the length of a descriptor after `>&` or `<&`.
fn descriptor_len(rest: &str) -> Option<usize> {
    let spaces = rest
        .len()
        .saturating_sub(rest.trim_start_matches([' ', '\t']).len());
    let after = rest.get(spaces..)?;
    let digits = after.bytes().take_while(u8::is_ascii_digit).count();
    let len = match (digits, after.starts_with('-')) {
        (0, true) => 1,
        (0, false) => return None,
        (d, _) => d,
    };
    let ends = after
        .get(len..)
        .and_then(|tail| tail.chars().next())
        .is_none_or(|c| is_js_space(c) || ";&|<>()".contains(c));
    ends.then(|| spaces.saturating_add(len))
}

/// 1.x `balanced(s, open, openCh, closeCh)`: the index of the closer that balances the opener
/// at `open`, skipping quoted text and escapes; `s.len()` when it is unbalanced.
pub(super) fn balanced(s: &str, open: usize, open_ch: u8, close_ch: u8) -> usize {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut depth: usize = 0;
    let mut i = open;
    while let Some(&c) = bytes.get(i) {
        match c {
            b'\\' => i = i.saturating_add(1),
            b'\'' => match s.get(i.saturating_add(1)..).and_then(|r| r.find('\'')) {
                Some(e) => i = i.saturating_add(1).saturating_add(e),
                None => return n,
            },
            b'"' => {
                let mut j = i.saturating_add(1);
                while let Some(&d) = bytes.get(j).filter(|&&d| d != b'"') {
                    j = j.saturating_add(if d == b'\\' { 2 } else { 1 });
                }
                i = j;
            }
            _ if c == open_ch => depth = depth.saturating_add(1),
            _ if c == close_ch => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i = i.saturating_add(1);
    }
    n
}

/// The backquoted substitution at `s[i]`, with `` \` ``, `\$` and `\\` unescaped in its
/// script, and the index after it.
fn backquoted(s: &str, i: usize, quoted: bool) -> (Part, usize) {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let slice = |from: usize, to: usize| s.get(from..to.min(n)).unwrap_or("");
    let mut j = i.saturating_add(1);
    let mut src = String::new();
    while let Some(&c) = bytes.get(j).filter(|&&c| c != b'`') {
        let next = bytes.get(j.saturating_add(1));
        if c == b'\\' && next.is_some_and(|d| b"`$\\".contains(d)) {
            src.push_str(slice(j.saturating_add(1), j.saturating_add(2)));
            j = j.saturating_add(2);
            continue;
        }
        let len = s
            .get(j..)
            .and_then(|r| r.chars().next())
            .map_or(1, char::len_utf8);
        src.push_str(slice(j, j.saturating_add(len)));
        j = j.saturating_add(len);
    }
    let part = Part {
        kind: PartKind::Sub(src),
        text: slice(i, j.saturating_add(1)).to_owned(),
        quoted,
    };
    (part, j.saturating_add(1).min(n))
}

/// 1.x `expansion(s, i, quoted)`: the expansion at `s[i]` (`$` or a backquote) and the index
/// after it, or `None` for a lone `$`.
fn expansion(s: &str, i: usize, quoted: bool) -> Option<(Part, usize)> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let slice = |from: usize, to: usize| s.get(from..to.min(n)).unwrap_or("").to_owned();
    if bytes.get(i) == Some(&b'`') {
        return Some(backquoted(s, i, quoted));
    }
    let part = |kind: PartKind, end: usize| Part {
        kind,
        text: slice(i, end.saturating_add(1)),
        quoted,
    };
    match bytes.get(i.saturating_add(1)) {
        Some(b'(') => {
            let e = balanced(s, i.saturating_add(1), b'(', b')');
            let kind = if bytes.get(i.saturating_add(2)) == Some(&b'(') {
                PartKind::Arith
            } else {
                PartKind::Sub(slice(i.saturating_add(2), e))
            };
            Some((part(kind, e), e.saturating_add(1)))
        }
        Some(b'{') => {
            let e = balanced(s, i.saturating_add(1), b'{', b'}');
            let inner = s.get(i.saturating_add(2)..e.min(n)).unwrap_or("");
            let kind = if is_name(inner) {
                PartKind::Var(inner.to_owned())
            } else {
                PartKind::Param
            };
            Some((part(kind, e), e.saturating_add(1)))
        }
        _ => {
            let name = var_name(s.get(i.saturating_add(1)..).unwrap_or(""))?;
            let end = i.saturating_add(1).saturating_add(name.len());
            let part = Part {
                kind: PartKind::Var(name.to_owned()),
                text: ["$", name].concat(),
                quoted,
            };
            Some((part, end))
        }
    }
}

/// `^[A-Za-z_]\w*$`.
pub(super) fn is_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The length of the `[A-Za-z_]\w*` run that starts `s`.
pub(super) fn name_len(s: &str) -> usize {
    match s.bytes().next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => s
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count(),
        _ => 0,
    }
}

/// 1.x `/^(?:env:[A-Za-z_]\w*|[A-Za-z_]\w*|[0-9@*#?$!-])/i` on the next 255 UTF-16 units: the
/// name of a `$NAME` expansion. A name is ASCII, so the window cuts it at 255 bytes.
fn var_name(after: &str) -> Option<&str> {
    const WINDOW: usize = 255;
    let env = after
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("env:"));
    let tail = name_len(after.get(4..).unwrap_or(""));
    let len = if env && tail > 0 {
        tail.saturating_add(4)
    } else {
        match name_len(after) {
            0 => usize::from(
                after
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_digit() || b"@*#?$!-".contains(&b)),
            ),
            len => len,
        }
    };
    (len > 0).then(|| after.get(..len.min(WINDOW))).flatten()
}

/// 1.x `ANSI_C`: the escapes of `$'...'` that stand for one char.
fn ansi_c_escape(c: char) -> Option<char> {
    Some(match c {
        'a' => '\x07',
        'b' => '\x08',
        'e' | 'E' => '\x1b',
        'f' => '\x0c',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'v' => '\x0b',
        '\\' | '\'' | '"' | '?' => c,
        _ => return None,
    })
}

/// 1.x `ansiC(s, j)`: the text of a `$'...'` whose body starts at `j`, decoded as bash
/// decodes it, and the index of its closing quote. The text is `None` when an escape is one
/// temper does not decode exactly: `\c`, a NUL, or a char outside ASCII.
fn ansi_c(s: &str, mut j: usize) -> (Option<String>, usize) {
    let n = s.len();
    let mut text = String::new();
    let mut exact = true;
    while let Some(c) = s
        .get(j..)
        .and_then(|r| r.chars().next())
        .filter(|&c| c != '\'')
    {
        let after = j.saturating_add(c.len_utf8());
        let escape = s.get(after..).and_then(|r| r.chars().next());
        let Some(e) = escape.filter(|_| c == '\\') else {
            text.push(c);
            j = after;
            continue;
        };
        if let Some(decoded) = ansi_c_escape(e) {
            text.push(decoded);
            j = after.saturating_add(e.len_utf8());
            continue;
        }
        if let Some((code, len)) = numeric_escape(s.get(after..).unwrap_or("")) {
            match char::from_u32(code).filter(|_| code != 0 && code <= 0x7f) {
                Some(ch) => text.push(ch),
                None => exact = false,
            }
            j = after.saturating_add(len);
            continue;
        }
        if "cxuU".contains(e) {
            exact = false;
        } else {
            // bash keeps an escape it does not know as written.
            text.push('\\');
            text.push(e);
        }
        j = after.saturating_add(e.len_utf8());
    }
    (exact.then_some(text), j.min(n))
}

/// 1.x `/^(?:[0-7]{1,3}|x[0-9a-fA-F]{1,2}|u[0-9a-fA-F]{1,4}|U[0-9a-fA-F]{1,8})/`: the code
/// and the length of a numeric escape after its backslash.
fn numeric_escape(s: &str) -> Option<(u32, usize)> {
    let digits = |from: usize, max: usize, radix: u32| {
        let run: String = s
            .get(from..)
            .unwrap_or("")
            .chars()
            .take(max)
            .take_while(|c| c.is_digit(radix))
            .collect();
        u32::from_str_radix(&run, radix)
            .ok()
            .map(|code| (code, from.saturating_add(run.len())))
    };
    match s.bytes().next()? {
        b'0'..=b'7' => digits(0, 3, 8),
        b'x' => digits(1, 2, 16),
        b'u' => digits(1, 4, 16),
        b'U' => digits(1, 8, 16),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{balanced, descriptor_len, var_name};

    #[test]
    fn helpers_read_as_1x_regexes_do() {
        assert_eq!(var_name("HOME/x"), Some("HOME"));
        assert_eq!(var_name("Env:TEMP/x"), Some("Env:TEMP"));
        assert_eq!(var_name("env:/x"), Some("env"));
        assert_eq!(var_name("1x"), Some("1"));
        assert_eq!(var_name("/x"), None);
        assert_eq!(var_name(&"a".repeat(300)).map(str::len), Some(255));
        assert_eq!(descriptor_len(" 1 x"), Some(2));
        assert_eq!(descriptor_len("-;"), Some(1));
        assert_eq!(descriptor_len("12x"), None);
        assert_eq!(descriptor_len("2\u{a0}"), Some(1));
        assert_eq!(balanced("(a ')' \"(\" (b))", 0, b'(', b')'), 14);
        assert_eq!(balanced("(a", 0, b'(', b')'), 2);
    }
}
