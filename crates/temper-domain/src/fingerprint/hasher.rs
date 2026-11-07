//! The fingerprint's SHA-256 over the frames 1.x feeds it (`project.mjs` `fingerprint`).

use std::io;

use sha2::{Digest, Sha256};

use super::Fingerprint;
use crate::js::LossyUtf8;

/// 1.x's `createHash('sha256')` in `fingerprint`, fed frame by frame in 1.x's order.
///
/// The frames are the head and revision, the diff (decoded lossily, as 1.x hashes git's stdout
/// after `encoding: 'utf8'`), each untracked or walked file's name and content, then the gate
/// config.
///
/// # Examples
///
/// ```
/// use std::io::Write as _;
/// use temper_domain::FingerprintHasher;
///
/// let mut hasher = FingerprintHasher::new();
/// hasher.head_and_rev(Some("abc"), Some("abc"));
/// let mut diff = hasher.diff();
/// diff.write_all(b"diff --git a/src/lib.rs b/src/lib.rs\n")?;
/// diff.finish();
/// let mut content = hasher.untracked("src/new.rs");
/// content.write_all(b"pub fn f() {}\n")?;
/// hasher.accept(content);
/// assert_eq!(hasher.finish().to_string().len(), 16);
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct FingerprintHasher(Sha256);

impl FingerprintHasher {
    /// A hasher that has seen nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 1.x `head:${head ?? ''}\nrev:${rev ?? ''}\n`.
    pub fn head_and_rev(&mut self, head: Option<&str>, rev: Option<&str>) {
        for part in [
            "head:",
            head.unwrap_or(""),
            "\nrev:",
            rev.unwrap_or(""),
            "\n",
        ] {
            self.0.update(part);
        }
    }

    /// A sink for `git diff`'s stdout, decoded as Node decodes it, however it is split into
    /// writes. Call [`DiffSink::finish`] at its end.
    pub fn diff(&mut self) -> DiffSink<'_> {
        DiffSink {
            hash: &mut self.0,
            decoder: LossyUtf8::default(),
        }
    }

    /// Hashes an untracked file's frame, `\n?<rel>\n`, and returns a sink for its content.
    /// Only a sink handed back through [`accept`](Self::accept) counts: one dropped (a read
    /// that failed part way) leaves the frame alone, as 1.x's `catch { /* vanished */ }`.
    pub fn untracked(&mut self, rel: &str) -> ContentSink {
        self.frame(&["\n?", rel, "\n"])
    }

    /// Hashes a walked file's frame outside git, `\n<rel>\n`, and returns a sink for its
    /// content, as [`untracked`](Self::untracked).
    pub fn file(&mut self, rel: &str) -> ContentSink {
        self.frame(&["\n", rel, "\n"])
    }

    /// Keeps the content written to `content`, whose file was read whole.
    pub fn accept(&mut self, content: ContentSink) {
        self.0 = content.0;
    }

    /// 1.x `\n.claude/temper.json\n${config}`, for a gate config that changed.
    pub fn gate_config(&mut self, canonical_json: &str) {
        for part in ["\n", super::TEMPER_CONFIG_FILE, "\n", canonical_json] {
            self.0.update(part);
        }
    }

    /// The fingerprint: 1.x's `digest('hex').slice(0, 16)`.
    #[must_use]
    pub fn finish(self) -> Fingerprint {
        Fingerprint::from_sha256(self.0.finalize().into())
    }

    fn frame(&mut self, parts: &[&str]) -> ContentSink {
        for part in parts {
            self.0.update(part);
        }
        // The content goes into a copy of the state, so a read that fails part way leaves the
        // hash as it was after the frame.
        ContentSink(self.0.clone())
    }
}

/// Where `git diff`'s stdout goes: decoded lossily as it streams, so a sequence split between
/// two writes decodes whole. Writes never fail.
#[derive(Debug)]
pub struct DiffSink<'h> {
    hash: &'h mut Sha256,
    decoder: LossyUtf8,
}

impl DiffSink<'_> {
    /// Ends the diff: a sequence left unfinished is one U+FFFD, as at the end of Node's buffer.
    /// Nothing can fail here (ERR-13).
    pub fn finish(self) {
        let Self { hash, decoder } = self;
        decoder.finish(&mut |bytes: &[u8]| hash.update(bytes));
    }
}

impl io::Write for DiffSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Self { hash, decoder } = self;
        decoder.feed(buf, &mut |bytes: &[u8]| hash.update(bytes));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A file's content on its way into the fingerprint, hashed raw, as 1.x hashes the `Buffer`
/// `readFileSync` returns. Writes never fail; see [`FingerprintHasher::accept`].
#[derive(Debug, Clone)]
pub struct ContentSink(Sha256);

impl io::Write for ContentSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
