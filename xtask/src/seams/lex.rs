//! Blanks what is not code in a Rust source, so the path scan reads only code.

/// The characters of `text` with comments (line, and nested block) and the contents of string,
/// raw string, byte string and char literals replaced by spaces. Newlines are kept, so line
/// numbers still hold, and quotes are kept, so no two tokens join.
pub(crate) fn code_only(text: &str) -> Vec<char> {
    let src: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while let Some(&c) = src.get(i) {
        let next = src.get(i.saturating_add(1)).copied();
        let end = match (c, next) {
            ('/', Some('/')) => Some(line_comment(&src, i)),
            ('/', Some('*')) => Some(block_comment(&src, i)),
            ('"', _) => Some(string(&src, i)),
            ('\'', _) => char_literal(&src, i),
            ('r' | 'b' | 'c', _) if starts_raw(&src, i) => Some(raw_string(&src, i)),
            _ => None,
        };
        if let Some(end) = end {
            blank(&src, i, end, &mut out);
            i = end;
        } else {
            out.push(c);
            i = i.saturating_add(1);
        }
    }
    out
}

/// Copies `src[from..to]` to `out` with every character but a newline or a quote made a space.
/// Quotes inside a literal are kept too; the path scan ignores quotes, so that is harmless.
fn blank(src: &[char], from: usize, to: usize, out: &mut Vec<char>) {
    out.extend(src.iter().take(to).skip(from).map(|&c| {
        if matches!(c, '\n' | '"' | '\'') {
            c
        } else {
            ' '
        }
    }));
}

fn line_comment(src: &[char], at: usize) -> usize {
    let len = src.iter().skip(at).take_while(|&&c| c != '\n').count();
    at.saturating_add(len)
}

/// The end of the block comment opening at `at`; block comments nest. An unclosed one runs to
/// the end of the text.
fn block_comment(src: &[char], at: usize) -> usize {
    let mut depth = 0_usize;
    let mut i = at;
    while let Some(&c) = src.get(i) {
        let next = src.get(i.saturating_add(1)).copied();
        match (c, next) {
            ('/', Some('*')) => {
                depth = depth.saturating_add(1);
                i = i.saturating_add(2);
            }
            ('*', Some('/')) => {
                depth = depth.saturating_sub(1);
                i = i.saturating_add(2);
                if depth == 0 {
                    return i;
                }
            }
            _ => i = i.saturating_add(1),
        }
    }
    i
}

/// The end of the string literal whose opening quote is at `at`, past its closing quote.
fn string(src: &[char], at: usize) -> usize {
    let mut i = at.saturating_add(1);
    while let Some(&c) = src.get(i) {
        match c {
            '\\' => i = i.saturating_add(2),
            '"' => return i.saturating_add(1),
            _ => i = i.saturating_add(1),
        }
    }
    i
}

/// The end of the char literal at `at`, or `None` when the quote opens a lifetime or a label.
fn char_literal(src: &[char], at: usize) -> Option<usize> {
    let first = src.get(at.saturating_add(1)).copied()?;
    if first == '\\' {
        // The escaped character itself, which may be a quote, then the rest up to the quote.
        let len = src
            .iter()
            .skip(at.saturating_add(3))
            .take_while(|&&c| c != '\'' && c != '\n')
            .count();
        return Some(at.saturating_add(len).saturating_add(4));
    }
    (src.get(at.saturating_add(2)) == Some(&'\'')).then_some(at.saturating_add(3))
}

/// Where the `r` of a raw string starting at `at` would be: past a `b` or `c` prefix.
fn raw_r(src: &[char], at: usize) -> usize {
    if matches!(src.get(at), Some('b' | 'c')) {
        at.saturating_add(1)
    } else {
        at
    }
}

/// Whether a raw string (`r"`, `r#"`, and the `br` and `cr` forms) starts at `at`, not inside an
/// identifier.
fn starts_raw(src: &[char], at: usize) -> bool {
    let inside_ident = at
        .checked_sub(1)
        .and_then(|p| src.get(p))
        .is_some_and(|&p| p.is_alphanumeric() || p == '_');
    if inside_ident {
        return false;
    }
    let r = raw_r(src, at);
    if src.get(r) != Some(&'r') {
        return false;
    }
    let hashes = src
        .iter()
        .skip(r.saturating_add(1))
        .take_while(|&&c| c == '#')
        .count();
    src.get(r.saturating_add(1).saturating_add(hashes)) == Some(&'"')
}

/// The end of the raw string at `at`, past its closing quote and hashes.
fn raw_string(src: &[char], at: usize) -> usize {
    let r = raw_r(src, at);
    let hashes = src
        .iter()
        .skip(r.saturating_add(1))
        .take_while(|&&c| c == '#')
        .count();
    let mut i = r.saturating_add(hashes).saturating_add(2);
    while let Some(&c) = src.get(i) {
        let closing = c == '"'
            && src
                .iter()
                .skip(i.saturating_add(1))
                .take(hashes)
                .filter(|&&h| h == '#')
                .count()
                == hashes;
        if closing {
            return i.saturating_add(1).saturating_add(hashes);
        }
        i = i.saturating_add(1);
    }
    i
}
