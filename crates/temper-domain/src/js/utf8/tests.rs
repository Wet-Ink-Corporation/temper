use proptest::prelude::*;

use super::LossyUtf8;

/// The decoder's output for `chunks`, fed in order, then finished.
fn decoded(chunks: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut decoder = LossyUtf8::default();
    for chunk in chunks {
        decoder.feed(chunk, &mut |bytes: &[u8]| out.extend_from_slice(bytes));
    }
    decoder.finish(&mut |bytes: &[u8]| out.extend_from_slice(bytes));
    out
}

const FFFD: [u8; 3] = [0xef, 0xbf, 0xbd];

/// Each row's answer is Node 22's `Buffer.from(Buffer.from(bytes).toString('utf8'))`.
fn node_rows() -> Vec<(Vec<u8>, Vec<u8>)> {
    let r = |n: usize| FFFD.repeat(n);
    vec![
        (vec![0xc0, 0x80], r(2)),
        (vec![0xe0, 0x80, 0x80], r(3)),
        (vec![0xed, 0xa0, 0x80], r(3)),
        (vec![0xf4, 0x90, 0x80, 0x80], r(4)),
        (vec![0xff], r(1)),
        (vec![0xe2, 0x82], r(1)),
        (vec![0xef, 0xbb, 0xbf, 0x41], vec![0xef, 0xbb, 0xbf, 0x41]),
        (
            vec![0x61, 0xf0, 0x9f, 0x98, 0x80, 0x62],
            vec![0x61, 0xf0, 0x9f, 0x98, 0x80, 0x62],
        ),
        (vec![0xe0, 0xa0], r(1)),
        (vec![0xf0, 0x9f, 0x98], r(1)),
        (vec![0xe2, 0x82, 0x41], [&FFFD[..], b"A"].concat()),
        (vec![0x80, 0x80], r(2)),
        (vec![0xf8, 0x88, 0x80, 0x80, 0x80], r(5)),
        (Vec::new(), Vec::new()),
    ]
}

#[test]
fn whole_inputs_decode_as_node_does() {
    for (input, expected) in node_rows() {
        assert_eq!(decoded(&[&input]), expected, "{input:x?}");
    }
}

#[test]
fn every_split_into_two_chunks_decodes_as_the_whole() {
    for (input, expected) in node_rows() {
        for at in 0..=input.len() {
            let (a, b) = input.split_at(at);
            assert_eq!(decoded(&[a, b]), expected, "{input:x?} split at {at}");
        }
    }
}

#[test]
fn one_byte_at_a_time_decodes_as_the_whole() {
    for (input, expected) in node_rows() {
        let chunks: Vec<&[u8]> = input.chunks(1).collect();
        assert_eq!(decoded(&chunks), expected, "{input:x?} byte by byte");
    }
}

/// P12: 64 MiB of invalid UTF-8 is one replacement per byte, decoded in one pass.
#[test]
fn a_long_run_of_invalid_bytes_is_one_replacement_each() {
    let input = vec![0xff_u8; 1 << 20];
    let mut count = 0_usize;
    let mut decoder = LossyUtf8::default();
    decoder.feed(&input, &mut |bytes: &[u8]| {
        count = count.saturating_add(bytes.len());
    });
    decoder.finish(&mut |bytes: &[u8]| count = count.saturating_add(bytes.len()));
    assert_eq!(count, 3 << 20);
}

proptest! {
    /// WHATWG's decoder, which `String::from_utf8_lossy` implements, on any bytes split
    /// anywhere into three chunks (TST-06).
    #[test]
    fn chunked_decoding_equals_the_whole_lossy_decode(
        bytes in proptest::collection::vec(any::<u8>(), 0..64),
        cut1 in 0_usize..64,
        cut2 in 0_usize..64,
    ) {
        let a = cut1.min(bytes.len());
        let b = cut2.clamp(a, bytes.len());
        let (head, rest) = bytes.split_at(a);
        let (mid, tail) = rest.split_at(b - a);
        let expected = String::from_utf8_lossy(&bytes).into_owned().into_bytes();
        prop_assert_eq!(decoded(&[head, mid, tail]), expected);
    }

    /// Bytes drawn from the lead and continuation ranges, where the edge cases are.
    #[test]
    fn chunked_decoding_of_utf8_like_bytes_equals_the_whole(
        bytes in proptest::collection::vec(prop_oneof![0x80_u8..=0xbf, 0xc0_u8..=0xff, Just(b'a')], 0..32),
        cut in 0_usize..32,
    ) {
        let (head, tail) = bytes.split_at(cut.min(bytes.len()));
        let expected = String::from_utf8_lossy(&bytes).into_owned().into_bytes();
        prop_assert_eq!(decoded(&[head, tail]), expected);
    }
}
