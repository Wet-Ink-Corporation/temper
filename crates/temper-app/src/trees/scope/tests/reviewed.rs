//! 1.x `reviewedTree(proj, input, fp)`: the checkout whose current tree has the verdict's
//! fingerprint, over the fakes. The cli/it `pre_handback` tests run it with real git.

use std::borrow::Cow;

use temper_domain::JsValue;
use temper_domain::PathStyle::Posix;

use super::super::{Checkout, reviewed_tree};
use super::{
    H1, H2, H3, MAIN, SESSION, SID, WT, call_with, checkout, failed, ran, record, record_path,
    resolving, with_checkout, workspace,
};
use crate::changes::fingerprint;
use crate::engine::RunEnv;
use crate::fakes::{FakeFiles, FakePorts, FakeProcess};

const OTHER: &str = "/r/wt-b";

/// The worktree [`WT`], whose own scope starts at `r2` ([`H2`]) and widens to the session base
/// `B` ([`H1`]), and another checkout of the session, [`OTHER`], at `o` ([`H3`]). Each revision
/// resolves to itself, so each base gives its own fingerprint.
fn ports() -> FakePorts {
    let files = [WT, OTHER]
        .into_iter()
        .fold(FakeFiles::new(Posix), with_checkout)
        .with_file(&record_path(WT, "agent-a1"), &record(WT, "r2", 4))
        .with_file(&record_path(OTHER, "agent-b1"), &record(OTHER, "o", 2))
        .with_file(&format!("{SESSION}/base"), "B")
        .with_file(&format!("{SESSION}/start-fp"), "sfp\n");
    let process = checkout(checkout(FakeProcess::new(), WT), OTHER);
    let process = [("B", H1), ("r2", H2), (H1, H1), (H2, H2)]
        .into_iter()
        .fold(process, |p, (rev, sha)| resolving(p, WT, rev, sha));
    let process = resolving(resolving(process, OTHER, "o", H3), OTHER, H3, H3)
        .answering(&["git", "merge-base", "--is-ancestor", H1, H2], WT, ran(""))
        .otherwise(failed());
    FakePorts {
        process,
        ..FakePorts::new(files)
    }
}

fn fp_of(ports: &FakePorts, root: &str, base: &str) -> JsValue {
    let env = RunEnv::at(WT, Posix);
    let fp = fingerprint(ports, &env, &workspace(root), Some(base));
    JsValue::String(fp.to_string())
}

/// The root, base and checkout `reviewed_tree` gives for `fp`.
fn reviewed(ports: &FakePorts, fp: Option<&JsValue>) -> (String, Option<String>, Checkout) {
    let ws = workspace(WT);
    let tree = call_with(ports, WT, SID, |call| reviewed_tree(call, &ws, fp)).expect("no throw");
    assert!(
        matches!(
            (&tree.ws, tree.checkout),
            (Cow::Borrowed(_), Checkout::Here) | (Cow::Owned(_), Checkout::Other)
        ),
        "this checkout is borrowed, another owned"
    );
    (tree.ws.root.clone(), tree.scope.base, tree.checkout)
}

fn here(base: &str) -> (String, Option<String>, Checkout) {
    (WT.to_owned(), Some(base.to_owned()), Checkout::Here)
}

#[test]
fn the_own_base_then_the_widened_base_then_another_checkout_whose_fingerprint_matches() {
    let ports = ports();
    let own = fp_of(&ports, WT, H2);
    let widened = fp_of(&ports, WT, H1);
    let other = fp_of(&ports, OTHER, H3);
    assert_ne!(own, widened, "the fakes give each base its own fingerprint");
    assert_eq!(reviewed(&ports, Some(&own)), here(H2));
    assert_eq!(
        reviewed(&ports, Some(&widened)),
        here(H1),
        "the main agent's widened scope"
    );
    assert_eq!(
        reviewed(&ports, Some(&other)),
        (OTHER.to_owned(), Some(H3.to_owned()), Checkout::Other)
    );
}

#[test]
fn no_fingerprint_or_one_no_checkout_has_gives_the_own_base() {
    let ports = ports();
    for fp in [
        "\"\"",
        "0",
        "false",
        "null",
        "5",
        r#""0123456789abcdef""#,
        "[]",
    ] {
        let fp = JsValue::from_json(fp).expect("JSON");
        assert_eq!(reviewed(&ports, Some(&fp)), here(H2), "{fp:?}");
    }
    assert_eq!(reviewed(&ports, None), here(H2));
}

#[test]
fn the_main_checkout_reviews_itself_when_its_fingerprint_matches() {
    let files = with_checkout(FakeFiles::new(Posix), MAIN);
    let process = checkout(FakeProcess::new(), MAIN).otherwise(failed());
    let ports = FakePorts {
        process,
        ..FakePorts::new(files)
    };
    let env = RunEnv::at(MAIN, Posix);
    let ws = workspace(MAIN);
    let fp = JsValue::String(fingerprint(&ports, &env, &ws, None).to_string());
    let tree = call_with(&ports, MAIN, SID, |call| {
        reviewed_tree(call, &ws, Some(&fp))
    })
    .expect("no throw");
    assert_eq!((tree.scope.base, tree.checkout), (None, Checkout::Here));
}
