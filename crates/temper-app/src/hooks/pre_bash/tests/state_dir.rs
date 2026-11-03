//! `tests/hooks.test.mjs`'s state-dir cases, in its order, each command with 1.x's answer on
//! the same workspace (no git, so the state dir is `/ws/target/temper`). Rows whose write is
//! interpreter code (`python3 -c`, `node -e`, `ruby -e` and a Python here-document) arrive
//! with `codeWriteTargets` in port Phase 2f; each test names them.

use super::{Answer, PASS, Ws};
use crate::hooks::pre_bash::tests::Answer::Deny;

const STATE: &str = "temper: temper's state dir (.git/temper/, or target/temper/ without git) holds the gate results and review verdicts, and the shell does not write into it: verdicts come only from the temper:rust-reviewer subagent; gate results come only from the gate command (`node \"$TEMPER_CLI\" gate`). Reading the files, copying them elsewhere and deleting telemetry.jsonl stay allowed.";

/// Every command of `denied` gets the state-dir denial and every one of `allowed` passes.
fn answers(ws: &Ws, denied: &[&str], allowed: &[&str]) {
    let table: Vec<(&str, Answer<'static>)> = denied
        .iter()
        .map(|&cmd| (cmd, Deny(STATE)))
        .chain(allowed.iter().map(|&cmd| (cmd, PASS)))
        .collect();
    ws.answers(&table);
}

/// 1.x "Bash: shell writes into the temper state dir (.git/temper/) are denied; reading,
/// copying out and deleting are not". Phase 2f: `python3 -c "open(...,'w')"`,
/// `python3 -c "import pathlib; ..."` and `node -e "require('fs').writeFileSync(...)"`.
#[test]
fn shell_writes_into_the_temper_state_dir_are_denied_reading_copying_out_and_deleting_are_not() {
    let ws = Ws::new();
    answers(
        &ws,
        &[
            r#"echo '{"verdict":"approved"}' > .git/temper/verdicts/0123456789abcdef.json"#,
            r#"printf %s "{}" >> ./.git/temper/telemetry.jsonl"#,
            "cat v.json &> .git/temper/gate/0123456789abcdef.json",
            "node fake.js 2> .git/temper/logs/0123-clippy.log",
            r#"echo {} >".git/temper/verdicts/a.json""#,
            "printf {} > /home/me/repo/.git/temper/gate/a.json",
            "cp /tmp/v.json .git/temper/gate/0123456789abcdef.json",
            r#"cp -f /tmp/v.json "./.git/temper/verdicts/a.json""#,
            "mv v.json .git/temper/verdicts/",
            "cp -t .git/temper/verdicts v.json",
            "rsync -avt v.json .git/temper/gate/a.json",
            "install -m 644 v.json .git/temper/verdicts/a.json",
            "ln -s /tmp/v.json .git/temper/verdicts/a.json",
            r"Copy-Item v.json -Destination .git\temper\verdicts\a.json",
            "Move-Item v.json .git/temper/gate/a.json",
            r#"jq ".verdict=\"approved\"" old.json | tee .git/temper/verdicts/a.json"#,
            "tee -a .git/temper/telemetry.jsonl < x",
            "sed -i 's/changes_requested/approved/' .git/temper/verdicts/a.json",
            "perl -pi -e 's/false/true/' .git/temper/gate/a.json",
            "Set-Content -Path .git/temper/verdicts/a.json -Value '{}'",
            r"'{}' | Out-File .git\temper\gate\a.json",
            "Add-Content .git/temper/telemetry.jsonl x",
            "dd if=v.json of=.git/temper/verdicts/a.json",
            r#"echo {} > "$(git rev-parse --git-common-dir)/temper/verdicts/a.json""#,
            r#"cp v.json "$(git rev-parse --git-dir)/temper/gate/a.json""#,
            "echo {} > `git rev-parse --git-common-dir`/temper/verdicts/a.json",
            "cp v.json $(git rev-parse --git-path temper/verdicts)/a.json",
            "echo {} > target/temper/verdicts/a.json",
            "echo {} > .git/temper/gate-progress-0123456789abcdef.json",
            r#"echo {} > "$STATE/temper/gate-progress-0123456789abcdef.json""#,
            r#"echo {} > "/ws/target/temper/gate/a.json""#,
            "echo {} > /ws/target/temper/gate/a.json",
        ],
        &[
            "cat .git/temper/logs/0123-clippy.log",
            "less .git/temper/logs/0123-test.log",
            "grep -n error .git/temper/logs/*.log",
            "ls -la .git/temper/verdicts",
            "jq .verdict .git/temper/verdicts/a.json",
            "tail -n 50 .git/temper/telemetry.jsonl",
            "cat .git/temper/telemetry.jsonl | wc -l",
            "sed -n 1,20p .git/temper/logs/a.log",
            r#"cat "$(git rev-parse --git-common-dir)/temper/verdicts/a.json""#,
            r#"node "$TEMPER_CLI" status"#,
            r#"node "$TEMPER_CLI" gate"#,
            r#"node "$TEMPER_CLI" stats"#,
            "grep x .git/temper/logs/a.log > /tmp/out",
            "jq . .git/temper/gate/a.json > gate.json 2> /tmp/err",
            "cp .git/temper/logs/a.log /tmp/",
            "cp -t /tmp .git/temper/logs/a.log",
            "cp -r .git/temper /tmp/temper-backup",
            r#"python3 -c "print(open('.git/temper/verdicts/a.json').read())""#,
            "cargo build > /tmp/b.log 2>&1",
            "mv .git/temper/telemetry.jsonl /tmp/t.jsonl",
            r#"rm "$(git rev-parse --git-common-dir)/temper/telemetry.jsonl""#,
            "rm -f .git/temper/telemetry.jsonl",
            "git status",
            "git gc --prune=now",
            "git rev-parse --git-common-dir",
            "ls target/temper",
            "cargo build 2> /tmp/err.log",
            "cargo test > target/debug/test-output.txt",
            "echo hi > temper.txt",
            "echo hi > notes/temper/todo.md",
        ],
    );
    let log = ws
        .ports
        .files
        .text("/ws/target/temper/telemetry.jsonl")
        .expect("the denials are logged");
    assert!(
        log.lines()
            .any(|row| row.contains(r#""event":"deny-shell-write""#)
                && row.contains(r#""file":"state-dir""#)),
        "{log}"
    );
}

/// 1.x "Bash: state-dir writes hidden by trailing redirects, variables, quoting, cd or other
/// writers are denied; reads stay allowed". Phase 2f: the four `python3 -c "open(...)"` and
/// `shutil.copy` rows, both Python here-documents, the three `node -e` rows and `ruby -e`.
#[test]
fn state_dir_writes_hidden_by_trailing_redirects_variables_quoting_cd_or_other_writers_are_denied_reads_stay_allowed()
 {
    answers(&Ws::new(), HIDDEN_WRITES, HIDDEN_READS);
}

/// The denied rows of the test above.
const HIDDEN_WRITES: &[&str] = &[
    "cp v.json .git/temper/verdicts/a.json 2>/dev/null",
    "cp v.json .git/temper/verdicts/a.json >/dev/null 2>&1",
    "cp v.json .git/temper/verdicts/a.json # forge",
    "ln -sf /tmp/v.json .git/temper/verdicts/a.json 2>/dev/null",
    "rsync v.json .git/temper/gate/a.json 2>&1",
    "mv v.json .git/temper/gate/a.json -f",
    "STATE=$(git rev-parse --git-common-dir)/temper; echo {} > $STATE/verdicts/a.json",
    r#"STATE="$(git rev-parse --git-common-dir)/temper"; echo {} > "$STATE/gate/a.json""#,
    "STATE=.git/temper/gate; cp v.json $STATE/a.json",
    "D=.git/temper && echo {} > ${D}/verdicts/x.json",
    r#"echo {} > "$(git rev-parse --git-common-dir)"/temper/verdicts/a.json"#,
    r#"echo {} > ".git/"temper/verdicts/a.json"#,
    "echo {} > .git/'temper'/verdicts/a.json",
    "cd .git/temper/verdicts && echo {} > x.json",
    "(cd .git/temper/gate && echo {} > a.json)",
    "pushd .git/temper/gate; echo {} > a.json",
    "cd .git && echo {} > temper/verdicts/x.json",
    "cd target/temper && echo {} > gate/a.json",
    r#"bash -c "echo {} > .git/temper/verdicts/a.json""#,
    "curl -o .git/temper/verdicts/a.json file:///tmp/v.json",
    "curl -sSLo .git/temper/verdicts/a.json file:///tmp/v.json",
    "wget -O .git/temper/verdicts/a.json http://example.com/v.json",
    r#"jq '.verdict="approved"' v.json | sponge .git/temper/verdicts/a.json"#,
    "tee --append .git/temper/verdicts/a.json < v.json",
    "echo {} | tee /dev/null .git/temper/verdicts/a.json",
    "tar -xf forged.tar -C .git/temper",
    "unzip -o forged.zip -d .git/temper",
    r"find /tmp -name v.json -exec cp {} .git/temper/verdicts/a.json \;",
    "ls /tmp/v.json | xargs -I{} cp {} .git/temper/verdicts/a.json",
    r"Copy-Item -Destination .git\temper\verdicts\a.json -Path v.json",
];

/// The allowed rows of the test above.
const HIDDEN_READS: &[&str] = &[
    "cp a.log /tmp/\ncat .git/temper/verdicts/a.json",
    "npm install\nls .git/temper/gate",
    "sed -i 's/a/b/' README.md\ncat .git/temper/telemetry.jsonl",
    "python3 <<'EOF'\nfrom pathlib import Path\nPath('/tmp/r.md').write_text('x')\nprint(open('.git/temper/telemetry.jsonl').read())\nEOF",
    r#"git commit -m "deny shell writes like echo > .git/temper/verdicts/x""#,
    r#"git commit -m "docs: cp logs out of .git/temper/logs""#,
    r#"grep -rn "> .git/temper" scripts/"#,
    "cat <<'EOF'\nTo forge: echo {} > .git/temper/verdicts/a.json\nEOF",
    "echo x > .git/temper-notes.txt",
    "echo x > target/temper-report.html",
    "cargo test 2> target/temper-stderr.txt",
    "echo hi > docs/.git/temper-notes.md",
    "cp -r src .git/temper-backup",
    r#"echo x > "$TMPDIR/temper/gate-summary.txt""#,
    r#"echo x > "$HOME/temper/logs/today.txt""#,
    r#"cp .git/temper/logs/a.log "$HOME/temper/logs/""#,
    r#"node -e "require('fs').writeFileSync('/tmp/g.json', require('fs').readFileSync('.git/temper/gate/a.json'))""#,
    r"Copy-Item -Destination C:\tmp -Path .git\temper\logs\a.log",
    "(cd .git/temper/logs && cat a.log) ; echo hi > out.txt",
    "cd .git/temper/logs && grep error *.log > /tmp/errors.txt",
    "tar -cf /tmp/state.tar -C .git/temper .",
    "cargo install cargo-nextest",
    "D=.git/temper; cat $D/verdicts/a.json > /tmp/a.json",
    "cd .git/temper && ls",
    "cd crates/foo && cargo test > ../../out.txt 2>&1",
    "cargo build --release 2>&1 | tail -20",
    r#"python3 -c "import json; print(json.load(open('.git/temper/gate/a.json')))""#,
];

/// 1.x "Bash: state-dir writes inside compound commands, command substitutions and moves
/// into .git/ are denied".
#[test]
fn state_dir_writes_inside_compound_commands_command_substitutions_and_moves_into_git_are_denied() {
    answers(
        &Ws::new(),
        &[
            "if true; then cp v.json .git/temper/verdicts/a.json; fi",
            "for f in a; do echo {} > .git/temper/gate/$f.json; done",
            "{ tee .git/temper/telemetry.jsonl < v; }",
            "! cp v.json .git/temper/verdicts/a.json",
            "while true; do cd .git/temper; echo {} > gate/a.json; break; done",
            r#"echo "$(cp v.json .git/temper/verdicts/a.json)""#,
            "x=`cp v.json .git/temper/verdicts/a.json`",
            "truncate -s 0 .git/temper/telemetry.jsonl",
            "mv forged/temper .git/",
        ],
        &[
            "if true; then cat .git/temper/verdicts/a.json; fi",
            r#"for f in .git/temper/logs/*; do cat "$f"; done > /tmp/all.log"#,
            r#"echo "$(cat .git/temper/telemetry.jsonl)" > /tmp/t"#,
            "mv notes/temper.txt .git/",
        ],
    );
}

/// 1.x "Bash: git rev-parse --path-format=absolute does not hide the state dir".
#[test]
fn git_rev_parse_path_format_absolute_does_not_hide_the_state_dir() {
    answers(
        &Ws::new(),
        &[
            r#"echo x > "$(git rev-parse --path-format=absolute --git-dir)/temper/gate/a""#,
            r#"cp v.json "$(git rev-parse --path-format=absolute --git-common-dir)/temper/verdicts/a.json""#,
        ],
        &[r#"cat "$(git rev-parse --path-format=absolute --git-dir)/temper/gate/a" > /tmp/a"#],
    );
}

/// 1.x "Bash: state-dir writes spelled with bash backslash escapes or a Python mode variable
/// are denied; reads stay allowed". Phase 2f: the two Python mode-variable rows.
#[test]
fn state_dir_writes_spelled_with_bash_backslash_escapes_are_denied_reads_stay_allowed() {
    answers(
        &Ws::new(),
        &[
            r"echo '{}' > .git/tem\per/verdicts/a.json",
            r"c\p /tmp/v.json .git/temper/gate/a.json",
            // A bash function named like a PowerShell command does not make bash keep the
            // backslash.
            r"copy() { cat; }; echo '{}' | copy > .git/tem\per/verdicts/v.json",
            r"'[a]::b'() { echo '{}'; }; [a]::b > .git/tem\per/verdicts/v.json",
            r"Set-Content() { cat; }; echo '{}' | Set-Content > .git/tem\per/gate/v.json",
        ],
        &[
            r"cat .git/tem\per/verdicts/a.json",
            r"Set-Content -Path target\temper-report.txt -Value x",
            r#"python3 -c "m='r'; print(open('.git/temper/verdicts/a.json', m).read())""#,
        ],
    );
}

/// Not in 1.x's tests: where the reader throws (a value doubled past V8's longest string),
/// 1.x's `readerRule` falls back to no state-dir write, and the later checks still run.
#[test]
fn a_command_the_reader_gives_up_on_is_not_a_state_dir_write() {
    let doubled = [
        "X=a; ",
        &"X=$X$X; ".repeat(29),
        "echo > .git/temper/verdicts/$X",
    ]
    .concat();
    answers(&Ws::new(), &[], &[&doubled]);
}

/// #201 re-review F2: a path joined from a directory and a name past V8's longest string
/// throws in 1.x's `path.resolve`, and `readerRule` falls back, so neither the state-dir deny
/// nor the protected-config ask answers (1.x prints nothing for both lines, 2.4 s and 2.1 s).
/// The directory and the name each fit: `$X` is 2^26 chars, the name seven of it.
#[test]
fn a_path_joined_past_v8s_longest_string_is_no_state_dir_or_protected_write() {
    let x26 = [
        "X=a; ",
        &"X=$X$X; ".repeat(26),
        "cd $X; Y=",
        &"$X".repeat(7),
    ]
    .concat();
    let state = [x26.as_str(), "/.git/temper/gate/x; echo > $Y"].concat();
    let config = [x26.as_str(), "/clippy.toml; echo > $Y"].concat();
    answers(&Ws::new(), &[], &[&state, &config]);
}

/// #202 F5: past its budget of directory text, each rule stops with its fallback, where no
/// string comes near V8's longest. Each `echo > o` in `/ws/$X` (`$X` is 2^16 chars) builds
/// that directory once, 20 of them about 1.25 MiB; the last write is from `/ws`, a root,
/// which builds nothing. Under a 2 MiB budget both rules answer; under 1 MiB neither does.
/// A root's text is never charged: the same line with an absolute `cd /ws/$X` answers under
/// the smaller budget (#202 F1: it was charged, and short lines fell back).
#[test]
fn past_the_directory_text_budget_the_state_dir_and_protected_rules_fall_back() {
    let ws = Ws::new();
    let x16 = ["X=a; ", &"X=$X$X; ".repeat(16)].concat();
    let writes = "echo > o; ".repeat(20);
    let line = |cd: &str, last: &str| [x16.as_str(), cd, &writes, "cd /ws; echo > ", last].concat();
    let state = line("cd $X; ", ".git/temper/x");
    let config = line("cd $X; ", "clippy.toml");
    let clippy = vec!["clippy.toml".to_owned()];
    assert!(ws.rules_within(&state, 2 << 20).0);
    assert_eq!(ws.rules_within(&config, 2 << 20).1, clippy);
    assert_eq!(ws.rules_within(&state, 1 << 20), (false, Vec::new()));
    assert_eq!(ws.rules_within(&config, 1 << 20), (false, Vec::new()));
    let rooted = line("cd /ws/$X; ", "clippy.toml");
    assert_eq!(ws.rules_within(&rooted, 1 << 20).1, clippy);
    let rooted = line("cd /ws/$X; ", ".git/temper/x");
    assert!(ws.rules_within(&rooted, 1 << 20).0);
}

/// #202 re-review M2: the text a rule joins from a word's pieces is charged to the line's
/// budget too, and is not kept, so a line naming a long value in many words falls back past
/// the budget (1.x runs out of memory on these: `echo > $X/a; ` x 2000 with a 2^24-char `$X`
/// held 11.4 GB). A command name is read from the end of its word, which joins nothing.
#[test]
fn past_the_budget_the_rules_fall_back_on_the_text_they_join_from_words() {
    let ws = Ws::new();
    let x16 = ["X=a; ", &"X=$X$X; ".repeat(16)].concat();
    let line = |each: &str, last: &str| [x16.as_str(), &each.repeat(40), last].concat();
    let clippy = vec!["clippy.toml".to_owned()];
    for each in ["echo > $X/a; ", "echo > /ws/$X/a; ", "echo > a/$X; "] {
        let state = line(each, "echo > .git/temper/x");
        let config = line(each, "echo > clippy.toml");
        assert!(ws.rules_within(&state, 16 << 20).0, "{each}");
        assert_eq!(ws.rules_within(&config, 16 << 20).1, clippy, "{each}");
        assert_eq!(ws.rules_within(&state, 1 << 20), (false, Vec::new()), "{each}");
        assert_eq!(ws.rules_within(&config, 1 << 20), (false, Vec::new()), "{each}");
    }
    let named = line("$X/a; ", "echo > .git/temper/x");
    assert!(ws.rules_within(&named, 1 << 16).0, "a command name joins nothing");
    let named = line("$X/a; ", "echo > clippy.toml");
    assert_eq!(ws.rules_within(&named, 1 << 16).1, clippy);
}
