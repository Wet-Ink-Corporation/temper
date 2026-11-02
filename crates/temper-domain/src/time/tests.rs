use super::Timestamp;

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
