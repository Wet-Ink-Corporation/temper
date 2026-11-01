//! 1.x tests 16 to 18: silently discarded errors and the ignore count.

use super::{IGNORE_SRC, has, hits_of, scan, sev_of};
use crate::scan::{CodeKind, Hit};

#[test]
fn empty_if_let_err_finds_the_body_brace_by_depth_empty_err_arm_allows_keep_first_error() {
    let none = Vec::<Hit>::new();
    let first_error = "\nfn parse_all(items: &[&str]) -> Result<(), ParseError> {\n    let mut first = None;\n    for item in items {\n        match parse(item) {\n            Ok(v) => keep(v),\n            Err(e) if first.is_none() => first = Some(e),\n            Err(_) => {}\n        }\n    }\n    first.map_or(Ok(()), Err)\n}";
    // a binding in a nested match does not excuse the outer arm
    let nested = "fn f() {\n    match a() {\n        Ok(v) => match b(v) { Err(e) => log(e), Ok(()) => {} },\n        Err(_) => {}\n    }\n}";

    assert_eq!(
        has(
            "fn f() { if let Err(_) = tx.try_send(Event::Tick {}) { tracing::warn!(\"queue full\"); } }",
            "empty-if-let-err",
            CodeKind::Prod
        ),
        none,
        "a struct literal's braces in the scrutinee are skipped"
    );
    assert_eq!(
        has(
            "fn f() -> Result<(), E> { if let Err(_) = retry(|| {}) { return Err(E::Gave); } Ok(()) }",
            "empty-if-let-err",
            CodeKind::Prod
        ),
        none,
        "a closure body in the scrutinee is skipped"
    );
    assert_eq!(
        sev_of(
            "fn f() { if let Err(_) = retry(|| { work() }) {} }",
            "empty-if-let-err",
            CodeKind::Prod
        ),
        ["hard"],
        "the empty body after a closure"
    );
    assert_eq!(
        sev_of(
            "fn f() { if let Err(_) = map[&k].send(1) {   } }",
            "empty-if-let-err",
            CodeKind::Prod
        ),
        ["hard"],
        "a blank body is empty"
    );
    assert_eq!(
        has(first_error, "empty-err-arm", CodeKind::Prod),
        none,
        "another arm keeps the first error"
    );
    assert_eq!(
        sev_of(nested, "empty-err-arm", CodeKind::Prod),
        ["hard"],
        "a nested match's binding does not count"
    );
}

#[test]
fn empty_if_let_err_body_search_like_1x() {
    let cases: [(&str, &[&str]); 12] = [
        ("fn f() { if let Err(_) == x {} }", &[]),
        (
            "fn f() { if let Err(_) =\n= x {} if let Err(..) = g() {} }",
            &["H-10:hard@1", "H-10:hard@2"],
        ),
        ("fn f() { if let Err(_x) = g(S {}) {} }", &["H-10:hard@1"]),
        ("fn f() { if let Err(_) = g() { } else { h(); } }", &[]),
        ("fn f() { if let Err(_) = g() {}\n    else if x {} }", &[]),
        (
            "fn f() { if let Err(_) = g() {}elsewhere(); }",
            &["H-10:hard@1"],
        ),
        ("fn f() { if let Err(_) = (g)[0] {} }", &["H-10:hard@1"]),
        ("fn f() { if let Err(_) = a; {} }", &[]),
        ("fn f() { if let Err(_) = g() {", &["H-10:hard@1"]),
        (
            "fn f() { if let Err(_) = g() { /* c */ } }",
            &["H-10:hard@1"],
        ),
        ("fn f() { if let Err(_) = x }", &[]),
        ("if let Err(_) = x {}", &["H-10:hard@1"]),
    ];

    for (src, want) in cases {
        assert_eq!(
            hits_of(src, "empty-if-let-err", CodeKind::Prod),
            want,
            "{src}"
        );
    }
}

#[test]
fn empty_err_arm_binding_forms_like_1x() {
    let in_match = |arms: &str| format!("fn f() {{ match g() {{ {arms} }} }}");
    let excused = [
        "Err(ref e) => log(e), Err(_) => {}",
        "Err(mut e) => log(e), Err(_) => ()",
        "Err(e @ E::A) => log(e), Err(_) => {}",
        "Err(x) => {}, Err(_) => {}",
    ];
    let reported = [
        in_match("Err(_e) => log(), Err(_) => {}"),
        in_match("Err(E) => log(), Err(_) => {}"),
        in_match("Ok(_) => {} Err(..) => {}"),
        "fn f() { let c = || { Err(_) => {} }; }".to_owned(),
        "fn f() { Err(_) => {} }".to_owned(),
        "fn f() { match g(); { Err(e) => a, Err(_) => {} } }".to_owned(),
    ];

    for arms in excused {
        let src = in_match(arms);
        assert_eq!(
            hits_of(&src, "empty-err-arm", CodeKind::Prod),
            Vec::<String>::new(),
            "{src}"
        );
    }
    for src in reported {
        assert_eq!(
            hits_of(&src, "empty-err-arm", CodeKind::Prod),
            ["H-10:hard@1"],
            "{src}"
        );
    }
}

#[test]
fn empty_err_arm_match_head_window_counts_utf16_units() {
    let src = |c: char, n: usize| {
        let pad = c.to_string().repeat(n);
        format!(
            "fn f() {{\n    match g(/*{pad}*/) {{\n        Err(_) => {{}}\n        Err(e) => log(e),\n    }}\n}}"
        )
    };
    let cases = [('é', 286), ('é', 287), ('😀', 143), ('😀', 144)];

    let hits = cases.map(|(c, n)| hits_of(&src(c, n), "empty-err-arm", CodeKind::Prod));

    assert_eq!(
        hits,
        [
            vec![],
            vec!["H-10:hard@3".to_owned()],
            vec![],
            vec!["H-10:hard@3".to_owned()],
        ],
        "`match` falls out of the 300-unit window at 287 `é` and 144 `😀`, not at 144 `é`"
    );
}

#[test]
fn scan_reports_ignore_count() {
    assert_eq!(
        scan(IGNORE_SRC, CodeKind::Test).ignores(),
        3,
        "cfg_attr(miri, ignore) is not counted"
    );
}

#[test]
fn empty_err_arm_and_empty_if_let_err_silent_discards() {
    let none = Vec::<Hit>::new();
    // ledger-store-jsonl/src/store.rs
    let good = "\nfn load(path: PathBuf) -> Result<Vec<Recorded>, StoreError> {\n    let file = match File::open(&path) {\n        Ok(file) => file,\n        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),\n        Err(source) => return Err(StoreError::Read { path, source }),\n    };\n    if let Err(source) = file.lock_shared() {\n        return Err(StoreError::Read { path, source });\n    }\n    if let Err(_) = probe() {} else { warm_up(); }\n    Ok(Vec::new())\n}";

    let silent = [
        (
            "fn a(r: Result<u8, E>) { match r { Ok(v) => keep(v), Err(_) => {} } }",
            "empty-err-arm",
            "hard",
        ),
        (
            "fn a() { match f() { Ok(()) => (), Err(_e) => (), } }",
            "empty-err-arm",
            "hard",
        ),
        (
            "fn a() {\n    match rx.try_recv() {\n        Ok(v) => keep(v),\n        Err(..) => { /* nothing to do */ }\n    }\n}",
            "empty-err-arm",
            "hard",
        ),
        (
            "fn a() { if let Err(_) = tx.send(1) {} }",
            "empty-if-let-err",
            "hard",
        ),
        (
            "fn a() { if let Err(_e) = tx.send(1) {\n    // receiver gone\n} }",
            "empty-if-let-err",
            "hard",
        ),
        (
            "#[test]\nfn t() { if let Err(_) = tx.send(1) {} }",
            "empty-if-let-err",
            "soft",
        ),
    ];

    for (src, key, sev) in silent {
        assert_eq!(sev_of(src, key, CodeKind::Prod), [sev], "{key}: {src}");
    }
    assert_eq!(
        has(good, "empty-err-arm", CodeKind::Prod),
        none,
        "bound arms are not silent"
    );
    assert_eq!(
        has(good, "empty-if-let-err", CodeKind::Prod),
        none,
        "a bound `if let` and an `else` are not silent"
    );
}
