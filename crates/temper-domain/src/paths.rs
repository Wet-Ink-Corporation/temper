//! Path text the way 1.x reads it: which platform's rules apply, and the Windows verbatim
//! prefix that 1.x strips before comparing paths.

use std::borrow::Cow;

/// Which platform's path rules apply: Node's `path.win32` or `path.posix`. A value rather than
/// `cfg!`, so both are tested on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathStyle {
    /// `\` and `/` separate, drive letters and UNC roots, case-insensitive comparison.
    Windows,
    /// Only `/` separates.
    Posix,
}

/// Strips a Windows verbatim prefix, as 1.x's `unverbatim` does.
///
/// `\\?\C:\x` becomes `C:\x` and `\\?\UNC\host\share` becomes `\\host\share`. Either separator
/// is accepted in the prefix, and `UNC` in any ASCII case. Under [`PathStyle::Posix`] the path
/// is unchanged. Borrows unless it strips a `UNC` prefix.
///
/// # Examples
///
/// ```
/// use temper_domain::{PathStyle, unverbatim};
///
/// assert_eq!(unverbatim(r"\\?\C:\ws", PathStyle::Windows), r"C:\ws");
/// assert_eq!(unverbatim(r"\\?\UNC\h\s", PathStyle::Windows), r"\\h\s");
/// assert_eq!(unverbatim(r"\\?\C:\ws", PathStyle::Posix), r"\\?\C:\ws");
/// ```
#[must_use]
pub fn unverbatim(path: &str, style: PathStyle) -> Cow<'_, str> {
    let is_sep = |b: u8| b == b'\\' || b == b'/';
    match (style, path.as_bytes()) {
        (PathStyle::Windows, [first, second, b'?', third, rest @ ..])
            if is_sep(*first) && is_sep(*second) && is_sep(*third) =>
        {
            match rest.split_first_chunk::<4>() {
                Some(([unc @ .., sep], _)) if is_sep(*sep) && unc.eq_ignore_ascii_case(b"UNC") => {
                    // The 8 prefix bytes are ASCII, so 8 is a char boundary.
                    Cow::Owned(["\\\\", path.get(8..).unwrap_or("")].concat())
                }
                // The 4 prefix bytes are ASCII, so 4 is a char boundary.
                _ => Cow::Borrowed(path.get(4..).unwrap_or("")),
            }
        }
        (PathStyle::Windows | PathStyle::Posix, _) => Cow::Borrowed(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.x's `unverbatim` regex, run under Node 22.22.0 on each row (`paths.mjs`).
    #[test]
    fn unverbatim_like_1x() {
        let windows = [
            (r"\\?\C:\x", r"C:\x"),
            ("//?/C:/x", "C:/x"),
            (r"\\?\UNC\h\s\x", r"\\h\s\x"),
            ("//?/UNC/h/s", r"\\h/s"),
            (r"\\?\unc\h\s", r"\\h\s"),
            (r"\\.\C:\x", r"\\.\C:\x"),
            (r"\?\C:\x", r"\?\C:\x"),
            (r"C:\x", r"C:\x"),
            (r"\\?\UNCx", "UNCx"),
            (r"\\?\", ""),
        ];

        let wrong: Vec<(&str, PathStyle, String)> = windows
            .iter()
            .map(|&(path, want)| (path, PathStyle::Windows, want))
            .chain(
                windows
                    .iter()
                    .map(|&(path, _)| (path, PathStyle::Posix, path)),
            )
            .filter(|&(path, style, want)| unverbatim(path, style) != want)
            .map(|(path, style, _)| (path, style, unverbatim(path, style).into_owned()))
            .collect();

        assert_eq!(wrong, Vec::new(), "verbatim prefixes stripped as 1.x does");
    }
}
