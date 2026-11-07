use proptest::prelude::*;

use super::{ModTime, Timestamp};

/// Node 22.22.0's `new Date(ms).toISOString()` for each row.
#[test]
fn display_is_node_to_iso_string() {
    let vectors: [(i64, &str); 17] = [
        (0, "1970-01-01T00:00:00.000Z"),
        (-1, "1969-12-31T23:59:59.999Z"),
        (1, "1970-01-01T00:00:00.001Z"),
        (999, "1970-01-01T00:00:00.999Z"),
        (1000, "1970-01-01T00:00:01.000Z"),
        (86_399_999, "1970-01-01T23:59:59.999Z"),
        (86_400_000, "1970-01-02T00:00:00.000Z"),
        (951_782_400_000, "2000-02-29T00:00:00.000Z"),
        (1_730_000_000_123, "2024-10-27T03:33:20.123Z"),
        (253_402_300_799_999, "9999-12-31T23:59:59.999Z"),
        (253_402_300_800_000, "+010000-01-01T00:00:00.000Z"),
        (-62_135_596_800_000, "0001-01-01T00:00:00.000Z"),
        (-62_135_596_800_001, "0000-12-31T23:59:59.999Z"),
        (-62_198_755_200_000, "-000001-01-01T00:00:00.000Z"),
        (8_640_000_000_000_000, "+275760-09-13T00:00:00.000Z"),
        (-8_640_000_000_000_000, "-271821-04-20T00:00:00.000Z"),
        (-1_000_000_000_000_000, "-029719-04-05T22:13:20.000Z"),
    ];

    let got: Vec<(i64, String)> = vectors
        .iter()
        .map(|&(ms, _)| (ms, Timestamp::from_unix_millis(ms).to_string()))
        .collect();
    let want: Vec<(i64, String)> = vectors
        .iter()
        .map(|&(ms, iso)| (ms, iso.to_owned()))
        .collect();

    assert_eq!(got, want, "as Node's toISOString prints them");
}

#[test]
fn millis_outside_a_js_date_are_clamped_to_its_range() {
    let clamped = [i64::MAX, i64::MIN, 8_640_000_000_000_001]
        .map(|ms| Timestamp::from_unix_millis(ms).unix_millis());

    assert_eq!(
        clamped,
        [
            8_640_000_000_000_000,
            -8_640_000_000_000_000,
            8_640_000_000_000_000
        ],
        "ECMA-262's TimeClip range"
    );
}

/// Node 22's `Date.parse` (run under `TZ=Asia/Kolkata`, so a local-time reading would show).
const ECMA_FORMS: [(&str, i64); 29] = [
    ("1970-01-01T00:00:00.000Z", 0),
    ("2024-10-27T03:33:20.123Z", 1_730_000_000_123),
    ("2026", 1_767_225_600_000),
    ("2026-03", 1_772_323_200_000),
    ("2026-03-04", 1_772_582_400_000),
    ("+002026", 1_767_225_600_000),
    ("-002026", -126_101_577_600_000),
    ("+002026-01-01", 1_767_225_600_000),
    ("-000001-01-01T00:00:00Z", -62_198_755_200_000),
    ("0000-01-01T00:00:00Z", -62_167_219_200_000),
    ("9999-12-31T23:59:59.999Z", 253_402_300_799_999),
    ("2026-03-04T05:06Z", 1_772_600_760_000),
    ("2026-03-04T05:06:07Z", 1_772_600_767_000),
    ("2026-03-04T05:06:07.8Z", 1_772_600_767_800),
    ("2026-03-04T05:06:07.89Z", 1_772_600_767_890),
    ("2026-03-04T05:06:07.8919Z", 1_772_600_767_891),
    ("2026-03-04T05:06:07.123456789012Z", 1_772_600_767_123),
    ("2026-03-04T05:06:07.891+01:30", 1_772_595_367_891),
    ("2026-03-04T05:06:07-01:30", 1_772_606_167_000),
    ("2026-03-04T05:06:07.891-00:00", 1_772_600_767_891),
    ("2026-03-04T05:06:07+23:59", 1_772_514_427_000),
    ("2026-03-04T24:00Z", 1_772_668_800_000),
    ("2026-03-04T24:00:00.000Z", 1_772_668_800_000),
    ("2026-02-30T00:00:00Z", 1_772_409_600_000),
    ("2026-04-31", 1_777_593_600_000),
    ("2024-02-29", 1_709_164_800_000),
    ("+275760-09-13T00:00:00.000Z", 8_640_000_000_000_000),
    ("-271821-04-20T00:00:00.000Z", -8_640_000_000_000_000),
    ("1969-12-31T23:59:59.999Z", -1),
];

#[test]
fn parse_js_reads_the_ecma_forms_as_node_does() {
    let got: Vec<(&str, Option<i64>)> = ECMA_FORMS
        .iter()
        .map(|&(text, _)| (text, Timestamp::parse_js(text).map(Timestamp::unix_millis)))
        .collect();
    let want: Vec<(&str, Option<i64>)> = ECMA_FORMS
        .iter()
        .map(|&(text, ms)| (text, Some(ms)))
        .collect();

    assert_eq!(got, want, "Node's Date.parse");
}

#[test]
fn parse_js_is_none_where_node_gives_nan() {
    let nan = [
        "",
        "x",
        "2026-13-01",
        "2026-00-01",
        "2026-01-00",
        "2026-01-32",
        "-000000-01-01T00:00:00Z",
        "+275760-09-13T00:00:00.001Z",
        "-271821-04-19T23:59:59.999Z",
        "2026-03-04T24:00:01Z",
        "2026-03-04T24:00:00.001Z",
        "2026-03-04T05:60:00Z",
        "2026-03-04T05:06:60Z",
        "2026-03-04T05:06:07.Z",
        "2026-03-04T05:06:07+24:00",
        "2026-03-04T05:06:07+01:60",
        "2026-03-04T5:06:07Z",
        "20260304",
        "2026-03-04TZ",
        "2026-03-04T05Z",
        "2026-03-04T05:06:07.000Z ",
        "2026-03-04T05:06:07.891+01",
        "\u{ff12}\u{ff10}\u{ff12}\u{ff16}",
    ];

    let parsed: Vec<&str> = nan
        .into_iter()
        .filter(|text| Timestamp::parse_js(text).is_some())
        .collect();

    assert_eq!(parsed, Vec::<&str>::new(), "each is NaN in Node");
}

/// OC-3 A (wi-06100e): V8 reads these through its local-time or legacy parser (the value in
/// the comment is Node's under `TZ=Asia/Kolkata`); temper reads them as NaN.
#[test]
fn parse_js_reads_local_and_legacy_forms_as_none() {
    let legacy = [
        "2026-03-04T05:06",              // 1772580960000, local time
        "2026-03-04T05:06:07",           // 1772580967000, local time
        "2026-03-04 05:06:07Z",          // 1772600767000
        "2026-03-04t05:06:07z",          // 1772600767000
        "2026-03-04T05:06:07.891+0130",  // 1772595367891
        "2026-03-04Z",                   // 1772582400000
        "2026Z",                         // 1767225600000
        "0",                             // 946665000000
        "5",                             // 988655400000
        "2026-3-4",                      // 1772562600000
        " 2026-03-04",                   // 1772562600000
        "Wed, 04 Mar 2026 05:06:07 GMT", // 1772600767000
        "March 4, 2026",                 // 1772562600000
    ];

    let parsed: Vec<&str> = legacy
        .into_iter()
        .filter(|text| Timestamp::parse_js(text).is_some())
        .collect();

    assert_eq!(
        parsed,
        Vec::<&str>::new(),
        "OC-3 A: outside ECMA-262's format"
    );
}

proptest! {
    /// What `toISOString` writes, `Date.parse` reads back: the forms temper itself records.
    #[test]
    fn parse_js_reads_back_every_iso_string(ms in -8_640_000_000_000_000_i64..=8_640_000_000_000_000) {
        let t = Timestamp::from_unix_millis(ms);

        prop_assert_eq!(Timestamp::parse_js(&t.to_string()), Some(t));
    }

    /// No input panics, and whatever reads is inside a JS `Date`'s range.
    #[test]
    fn parse_js_never_leaves_the_date_range(text in "[-+0-9T:.Z]{0,40}") {
        let ms = Timestamp::parse_js(&text).map_or(0, Timestamp::unix_millis);

        prop_assert!(ms.unsigned_abs() <= 8_640_000_000_000_000);
    }
}

/// Node 22's `fs.statSync(p).mtimeMs` for files touched to each time (`touch -d @<s>`), with
/// the nanoseconds from `{ bigint: true }`.
#[test]
fn mod_time_is_node_mtime_ms() {
    let vectors: [(i128, f64); 6] = [
        (1_730_000_000_123_456_789, 1_730_000_000_123.456_8),
        (-1_500_000_000, -1500.0),
        (-1, -9.999_999_974_752_427e-7),
        (500, 0.0005),
        (1_700_000_000_999_999_999, 1_700_000_001_000.0),
        (-86_400_250_000_000, -86_400_250.0),
    ];

    let got: Vec<(i128, u64)> = vectors
        .iter()
        .map(|&(ns, _)| (ns, ModTime::from_unix_nanos(ns).millis().to_bits()))
        .collect();
    let want: Vec<(i128, u64)> = vectors.iter().map(|&(ns, ms)| (ns, ms.to_bits())).collect();

    assert_eq!(got, want, "bit for bit, as Node's double");
}
