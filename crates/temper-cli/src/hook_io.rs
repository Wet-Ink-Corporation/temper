//! Reading a hook's input from stdin, as 1.x's `readStdin` does, but bounded (H-13).

use std::io::Read;

/// The most stdin a hook reads: 256 MiB, half of V8's longest string (port plan D16(n)).
pub(crate) const STDIN_CAP: usize = 256 * 1024 * 1024;

/// The input could not be read; the hook answers as for input that is not JSON (D16(n)), and
/// 1.x's notice names no cause, so none is kept.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputError {
    /// More than the cap.
    TooLarge,
    /// The stream failed.
    Read,
}

/// All of `reader`, decoded as lossy UTF-8 like Node's `Buffer#toString('utf8')`, or
/// [`InputError::TooLarge`] past `cap` bytes; at most `cap + 1` bytes are read.
pub(crate) fn read_input<R: Read>(reader: R, cap: usize) -> Result<String, InputError> {
    let limit = u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_unread| InputError::Read)?;
    if bytes.len() > cap {
        return Err(InputError::TooLarge);
    }
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(invalid) => String::from_utf8_lossy(invalid.as_bytes()).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{InputError, read_input};

    #[test]
    fn input_is_decoded_lossily_up_to_the_cap() {
        let read = |bytes: &[u8], cap| match read_input(bytes, cap) {
            Ok(text) => Ok(text),
            Err(InputError::TooLarge) => Err("too large"),
            Err(InputError::Read) => Err("read"),
        };

        assert_eq!(
            [
                read(b"{}", 2),
                read(b"{} ", 2),
                read(b"", 0),
                read(&[0x7b, 0x80, 0x7d], 3),
            ],
            [
                Ok("{}".to_owned()),
                Err("too large"),
                Ok(String::new()),
                Ok("{\u{fffd}}".to_owned()),
            ],
            "Buffer#toString('utf8') within the cap"
        );
    }

    struct Broken;

    impl io::Read for Broken {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn a_failing_stream_is_a_read_error() {
        assert_eq!(
            read_input(Broken, 8),
            Err(InputError::Read),
            "the stream failed"
        );
    }
}
