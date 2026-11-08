use super::{
    CachedGate, OtherTree, ReviewerExtras, SubagentStartFacts, Target, last_lines,
    subagent_start_context, tree_gate_command,
};
use crate::{Fingerprint, GateRecord};

const BASE: &str = "1111111111111111111111111111111111111111";

fn fp(hex: &str) -> Fingerprint {
    hex.parse().expect("16 hex")
}

fn facts<'a>(gate: &'a CachedGate, changes: &'a [String]) -> SubagentStartFacts<'a> {
    SubagentStartFacts {
        agent_type: "rust-implementer",
        target: Target::Here,
        root: "/w",
        base: Some(BASE),
        fingerprint: fp("00000000000000aa"),
        changes,
        gate,
        gate_command: "node \"/p/scripts/temper.mjs\" gate",
        plans: &[],
        also: &[],
        reviewer: None,
    }
}

#[test]
fn the_implementer_gets_the_tree_its_scope_fingerprint_gate_and_plans() {
    let changes = ["crates/core/src/lib.rs".to_owned()];
    let plans = [
        ".temper/plans/a.md".to_owned(),
        ".temper/plans/b.md".to_owned(),
    ];
    let text = subagent_start_context(&SubagentStartFacts {
        plans: &plans,
        ..facts(&CachedGate::NotRun, &changes)
    });

    assert_eq!(
        text,
        concat!(
            "temper context for rust-implementer:\n",
            "- Project root: /w\n",
            "- Base revision: 1111111111111111111111111111111111111111; review scope: `git diff 1111111111111111111111111111111111111111` plus untracked files.\n",
            "- Current tree fingerprint: 00000000000000aa\n",
            "- Changed Rust files (1): crates/core/src/lib.rs\n",
            "- Gate for this fingerprint: not run yet. Gate command: node \"/p/scripts/temper.mjs\" gate\n",
            "- Plans: .temper/plans/a.md, .temper/plans/b.md",
        )
    );
}

#[test]
fn without_git_or_changes_the_lines_say_so() {
    let text = subagent_start_context(&SubagentStartFacts {
        base: None,
        ..facts(&CachedGate::NotRun, &[])
    });

    assert!(
        text.contains(
            "\n- Base revision: none (no git); review scope: `git diff ` plus untracked files.\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("\n- Changed Rust files (0): none\n"),
        "{text}"
    );
    assert!(
        text.ends_with("\n- Plans: none under .temper/plans/"),
        "{text}"
    );
}

#[test]
fn only_the_first_forty_changes_are_named() {
    let changes: Vec<String> = (0..45).map(|i| format!("f{i}.rs")).collect();
    let text = subagent_start_context(&facts(&CachedGate::NotRun, &changes));

    assert!(
        text.contains("- Changed Rust files (45): f0.rs, "),
        "{text}"
    );
    assert!(text.contains(", f39.rs\n"), "{text}");
    assert!(!text.contains("f40.rs"), "{text}");
}

#[test]
fn a_gate_record_reads_green_red_with_its_failed_steps_or_not_run() {
    let rows = [
        (r#"{"ok":true,"steps":"x"}"#, "green"),
        (
            r#"{"ok":false,"steps":[{"name":"fmt","ok":true},{"name":"clippy","ok":false},{"name":"test"}]}"#,
            "red (clippy, test)",
        ),
        // Printed by Node 22 (`.temper/probe3e/steps.mjs`).
        (
            r#"{"steps":[5,"x",[],{"name":null},{"name":["a",null,"b"]},{"name":7,"ok":0},{"ok":"yes","name":"z"}]}"#,
            "red (, , , , a,,b, 7)",
        ),
        (r#"{"steps":[{"name":{}}]}"#, "red ([object Object])"),
        (r#"{"steps":[]}"#, "red ()"),
        ("null", "not run yet"),
        ("0", "not run yet"),
    ];
    for (json, said) in rows {
        let record = GateRecord::from_json(json).expect("JSON");
        let gate = CachedGate::of(Some(&record)).expect("1.x reads it");
        let text = subagent_start_context(&facts(&gate, &[]));
        let want = format!("- Gate for this fingerprint: {said}. Gate command: ");
        assert!(text.contains(&want), "{json}: {text}");
    }
}

#[test]
fn a_red_gate_whose_steps_1x_cannot_filter_is_an_error() {
    for (json, why) in [
        (r#"{"steps":"abc"}"#, "not an array"),
        ("{}", "not an array"),
        (r#"{"steps":[null]}"#, "null"),
        (r#"{"steps":{"filter":1}}"#, "not an array"),
        (r#"{"steps":[{"name":{"toString":1}}]}"#, "toString"),
    ] {
        let record = GateRecord::from_json(json).expect("JSON");
        let gate = CachedGate::of(Some(&record));
        assert!(
            matches!(&gate, Err(error) if error.message().contains(why)),
            "{json}: {gate:?}"
        );
    }
}

#[test]
fn a_worktree_target_and_the_other_worktrees_are_spelled_out() {
    let gate = tree_gate_command("temper gate", Some("2222"), true);
    let other_gate = tree_gate_command("temper gate", Some("3333"), true);
    let also = [OtherTree {
        label: ".claude/worktrees/b",
        at: "/w/.claude/worktrees/b",
        fingerprint: fp("00000000000000bb"),
        rust_files: 2,
        base: Some("3333"),
        gate_command: &other_gate,
    }];
    let text = subagent_start_context(&SubagentStartFacts {
        agent_type: "temper:rust-reviewer",
        target: Target::Worktree {
            label: ".claude/worktrees/a",
            at: "/w/.claude/worktrees/a",
        },
        root: "/w/.claude/worktrees/a",
        base: Some("2222"),
        gate_command: &gate,
        also: &also,
        reviewer: Some(ReviewerExtras {
            earlier: Some("- An earlier review of this same tree is recorded: x"),
            diff_stat: " a.rs | 2 +-\n 1 file changed",
        }),
        ..facts(&CachedGate::NotRun, &[])
    });

    assert_eq!(
        text,
        concat!(
            "temper context for temper:rust-reviewer:\n",
            "- Target: the worktree .claude/worktrees/a. This session changed Rust there and not in the main checkout, so that worktree is the tree to work on: run every command in it (`git -C \"/w/.claude/worktrees/a\" …`, and the gate as `cd \"/w/.claude/worktrees/a\" && TEMPER_BASE=2222 temper gate --check`).\n",
            "- Project root: /w/.claude/worktrees/a\n",
            "- Base revision: 2222; review scope: `git -C \"/w/.claude/worktrees/a\" diff 2222` plus untracked files.\n",
            "- Current tree fingerprint: 00000000000000aa\n",
            "- Changed Rust files (0): none\n",
            "- Gate for this fingerprint: not run yet. Gate command: TEMPER_BASE=2222 temper gate --check\n",
            "- Plans: none under .temper/plans/\n",
            "- Also changed in this session: the worktree .claude/worktrees/b (fingerprint 00000000000000bb, 2 Rust file(s)). Its scope is `git -C \"/w/.claude/worktrees/b\" diff 3333` plus its untracked files, and its gate runs inside it (`cd \"/w/.claude/worktrees/b\" && TEMPER_BASE=3333 temper gate --check`); a review of it uses that fingerprint.\n",
            "- Your verdict's \"fingerprint\" field must be 00000000000000aa, or, if you were asked to review one of the worktrees listed above, that worktree's fingerprint. If the tree changes during your review the verdict is recorded as stale and a new review will be needed.\n",
            "- An earlier review of this same tree is recorded: x\n",
            "- Diff stat:\n a.rs | 2 +-\n 1 file changed",
        )
    );
}

#[test]
fn a_reviewer_alone_must_give_this_fingerprint_and_an_empty_stat_is_left_out() {
    let text = subagent_start_context(&SubagentStartFacts {
        reviewer: Some(ReviewerExtras {
            earlier: None,
            diff_stat: "",
        }),
        ..facts(&CachedGate::NotRun, &[])
    });

    assert!(text.ends_with("\n- Your verdict's \"fingerprint\" field must be 00000000000000aa. If the tree changes during your review the verdict is recorded as stale and a new review will be needed."), "{text}");
}

#[test]
fn the_diff_stat_keeps_its_last_25_lines() {
    let stat: Vec<String> = (1..=30).map(|i| format!("l{i}")).collect();
    let stat = stat.join("\n");

    assert_eq!(last_lines(&stat).lines().next(), Some("l6"));
    assert_eq!(last_lines(&stat).lines().count(), 25);
    assert_eq!(last_lines("a\nb"), "a\nb");
    let exactly: Vec<String> = (1..=25).map(|i| format!("l{i}")).collect();
    assert_eq!(last_lines(&exactly.join("\n")), exactly.join("\n"));
}
