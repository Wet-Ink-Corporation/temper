//! Node's lossy UTF-8 decoding (`Buffer#toString('utf8')`), as a stream.

/// U+FFFD in UTF-8.
const REPLACEMENT: &[u8] = "\u{fffd}".as_bytes();

/// A streaming form of Node's `Buffer#toString('utf8')`, the WHATWG decoder: each maximal
/// ill-formed subpart becomes one U+FFFD and a leading BOM is kept. It emits the decoded text's
/// UTF-8, which is what 1.x hashes after decoding (`h.update(string)`), so valid input passes
/// through untouched.
///
/// A sequence cut between two chunks is carried to the next (at most 3 bytes), so any split of
/// the input decodes as the whole does. Each byte is looked at a bounded number of times.
#[derive(Debug, Default)]
pub(crate) struct LossyUtf8 {
    /// The start of a sequence the last chunk ended inside: a valid prefix, 1 to 3 bytes.
    carry: Vec<u8>,
}

impl LossyUtf8 {
    /// Decodes `bytes`, handing each piece of output to `emit` in order.
    pub(crate) fn feed(&mut self, bytes: &[u8], emit: &mut impl FnMut(&[u8])) {
        let rest = self.complete_carry(bytes, emit);
        self.decode(rest, emit);
    }

    /// Ends the stream: a sequence left unfinished is one U+FFFD, as at the end of a buffer.
    pub(crate) fn finish(self, emit: &mut impl FnMut(&[u8])) {
        if !self.carry.is_empty() {
            emit(REPLACEMENT);
        }
    }

    /// Finishes the carried sequence from the front of `bytes`, and returns what is left of them.
    fn complete_carry<'b>(&mut self, bytes: &'b [u8], emit: &mut impl FnMut(&[u8])) -> &'b [u8] {
        let Some(&lead) = self.carry.first() else {
            return bytes;
        };
        let missing = sequence_len(lead).saturating_sub(self.carry.len());
        let (head, rest) = bytes.split_at(missing.min(bytes.len()));
        self.carry.extend_from_slice(head);
        match std::str::from_utf8(&self.carry) {
            Ok(_) => {
                emit(&self.carry);
                self.carry.clear();
                rest
            }
            Err(error) => match error.error_len() {
                // Still short of the sequence's length: `bytes` ran out.
                None => rest,
                Some(bad) => {
                    // The carry was a valid prefix, so the subpart ends inside `head`; the bytes
                    // of `head` after it are read again as the start of what follows.
                    emit(REPLACEMENT);
                    let reread = self.carry.len().saturating_sub(bad);
                    self.carry.clear();
                    let from = head.len().saturating_sub(reread);
                    // `from` is at most `head.len()`, a prefix length of `bytes`.
                    bytes.get(from..).unwrap_or_default()
                }
            },
        }
    }

    /// Decodes `bytes` with nothing carried, carrying an unfinished sequence at their end.
    fn decode(&mut self, mut bytes: &[u8], emit: &mut impl FnMut(&[u8])) {
        loop {
            let Err(error) = std::str::from_utf8(bytes) else {
                if !bytes.is_empty() {
                    emit(bytes);
                }
                return;
            };
            let (valid, after) = bytes.split_at(error.valid_up_to());
            if !valid.is_empty() {
                emit(valid);
            }
            let Some(bad) = error.error_len() else {
                self.carry.extend_from_slice(after);
                return;
            };
            emit(REPLACEMENT);
            // `error_len` counts bytes of `after`, so `bad <= after.len()`.
            bytes = after.get(bad..).unwrap_or_default();
        }
    }
}

/// The length of the sequence a valid lead byte starts (`0xC2..=0xF4`).
fn sequence_len(lead: u8) -> usize {
    match lead {
        0xf0.. => 4,
        0xe0.. => 3,
        _ => 2,
    }
}

#[cfg(test)]
mod tests;
