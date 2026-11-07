use std::io::Write as _;

use super::FingerprintHasher;

/// The fingerprint of the bytes the hasher was given, as text.
fn fp(hasher: FingerprintHasher) -> String {
    hasher.finish().to_string()
}

/// `printf '' | sha256sum`, first 16 hex.
#[test]
fn nothing_hashed_is_the_empty_streams_digest() {
    assert_eq!(fp(FingerprintHasher::new()), "e3b0c44298fc1c14");
}

/// `printf 'abc' | sha256sum`, first 16 hex (NIST's "abc").
#[test]
fn a_diff_of_abc_is_nists_abc_digest() {
    let mut hasher = FingerprintHasher::new();
    let mut diff = hasher.diff();
    diff.write_all(b"abc").expect("the sink never fails");
    diff.finish();
    assert_eq!(fp(hasher), "ba7816bf8f01cfea");
}

/// 1.x `head:${head ?? ''}\nrev:${rev ?? ''}\n` with neither: `printf 'head:\nrev:\n' | sha256sum`.
#[test]
fn absent_head_and_rev_hash_as_empty_text() {
    let mut hasher = FingerprintHasher::new();
    hasher.head_and_rev(None, None);
    assert_eq!(fp(hasher), "97bf913cfb3f6448");
}

/// Every frame kind, against `sha256sum` of
/// `head:h1\nrev:r1\nd\xef\xbf\xbd\n?tests/a.rs\nx\n?tests/b.rs\n\nsrc/c.rs\ny\n.claude/temper.json\n{"a":1}`:
/// the diff decoded lossily (`\xff` hashes as U+FFFD), an untracked file whose content was read
/// whole, one whose read failed (its frame only), a walked file, and the gate config.
#[test]
fn one_stream_of_every_frame_kind_hashes_as_sha256sum_says() {
    let mut hasher = FingerprintHasher::new();
    hasher.head_and_rev(Some("h1"), Some("r1"));
    let mut diff = hasher.diff();
    diff.write_all(b"d").expect("the sink never fails");
    diff.write_all(&[0xff]).expect("the sink never fails");
    diff.finish();
    let mut content = hasher.untracked("tests/a.rs");
    content.write_all(b"x").expect("the sink never fails");
    hasher.accept(content);
    let mut vanished = hasher.untracked("tests/b.rs");
    vanished.write_all(b"half of it").expect("the sink never fails");
    drop(vanished);
    let mut walked = hasher.file("src/c.rs");
    walked.write_all(b"y").expect("the sink never fails");
    hasher.accept(walked);
    hasher.gate_config(r#"{"a":1}"#);
    assert_eq!(fp(hasher), "775fc7befb0513c1");
}

/// A sequence cut between two writes is decoded whole, and one left dangling at the end is one
/// U+FFFD, as Node decodes the whole stdout at once.
#[test]
fn the_diff_decodes_across_writes_and_flushes_a_dangling_sequence() {
    let split = {
        let mut hasher = FingerprintHasher::new();
        let mut diff = hasher.diff();
        diff.write_all(&[b'd', 0xef, 0xbf]).expect("the sink never fails");
        diff.write_all(&[0xbd]).expect("the sink never fails");
        diff.finish();
        fp(hasher)
    };
    let dangling = {
        let mut hasher = FingerprintHasher::new();
        let mut diff = hasher.diff();
        diff.write_all(&[b'd', 0xef, 0xbf]).expect("the sink never fails");
        diff.finish();
        fp(hasher)
    };
    let whole = {
        let mut hasher = FingerprintHasher::new();
        let mut diff = hasher.diff();
        diff.write_all("d\u{fffd}".as_bytes()).expect("the sink never fails");
        diff.finish();
        fp(hasher)
    };
    assert_eq!([&split, &dangling], [&whole, &whole]);
}
