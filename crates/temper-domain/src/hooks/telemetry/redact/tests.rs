//! `redact_command`, ported from `tests/telemetry.test.mjs`'s `redactCommand` tests in their
//! order, with 1.x's inputs and expected texts. The time bound of "time does not grow with the
//! command" is retired into the hook parity harness's per-step budget (OC-4 A, wi-3ea323).

use super::Redactor;

fn redactor() -> Redactor {
    Redactor::new().expect("every redaction compiles")
}

/// Every `(command, redacted)` pair of `table`.
fn check(r: &Redactor, table: &[(&str, &str)]) {
    for &(cmd, want) in table {
        assert_eq!(r.redact(cmd), want, "{cmd}");
    }
}

#[test]
fn likely_secrets_become_stars_and_the_rest_of_the_command_stays_readable() {
    let r = redactor();
    check(
        &r,
        &[
            (
                "gh api --token ghp_abcdefghijklmnopqrstuvwx repos",
                "gh api --token *** repos",
            ),
            (
                "npm publish --password=hunter2 --otp 123",
                "npm publish --password=*** --otp 123",
            ),
            (
                "curl -H \"Authorization: Bearer abc.def\" https://x",
                "curl -H \"…\" https://x",
            ),
            (
                "curl -H 'X-Api-Key: k1' https://u:p4ss@host/x",
                "curl -H '…' https://***@host/x",
            ),
            (
                "git push https://oauth2:tok@github.com/o/r",
                "git push https://***@github.com/o/r",
            ),
            (
                "GH_TOKEN=abc123 cmd; export API_KEY='s e'; PATH=/bin ls",
                "GH_TOKEN=*** cmd; export API_KEY=***; PATH=/bin ls",
            ),
            (
                "DB_PASSWORD=\"a b\" AWS_SECRET_ACCESS_KEY=x run",
                "DB_PASSWORD=*** AWS_SECRET_ACCESS_KEY=*** run",
            ),
            ("curl -d {\"api_key\":\"zz\"} x", "curl -d {\"…\":\"…\"} x"),
            (
                "echo sk-abcdefghijklmnopqrstuvwxyz0123 xoxb-1234567890-abc AKIAABCDEFGHIJKLMNOP",
                "echo *** *** ***",
            ),
            (
                "GIT_AUTHOR_NAME=me git commit --author me -m x",
                "GIT_AUTHOR_NAME=me git commit --author me -m x",
            ),
            (
                "git push -u origin main && sort -u f && git checkout -b fix/token-refresh",
                "git push -u origin main && sort -u f && git checkout -b fix/token-refresh",
            ),
        ],
    );
}

#[test]
fn only_the_first_line_up_to_any_heredoc_or_here_string_is_kept() {
    let r = redactor();
    check(
        &r,
        &[
            (
                "cat > src/lib.rs <<EOF\nlet k = \"s3cret\";\nEOF",
                "cat > src/lib.rs <<…",
            ),
            (
                "gh auth login --with-token <<< tok_s3cret",
                "gh auth login --with-token ***…",
            ),
            ("cat <<< s3cret > f", "cat <<…"),
            ("echo a &&\r\necho s3cret", "echo a &&…"),
            (
                "curl \\\n  -H \"Authorization: Bearer s3cret\" x",
                "curl \\…",
            ),
        ],
    );
}

/// The secret forms of 1.x's test, each command with its redaction.
const SECRET_FORMS: &[(&str, &str)] = &[
    ("curl -u bob:s3cret https://x", "curl -u *** https://x"),
    (
        "curl --user=bob:s3cret x; curl --user \"bob:s3 cret\" x; curl -ubob:s3cret x",
        "curl --user=*** x; curl --user \"…\" x; curl -u*** x",
    ),
    (
        "$env:GH_TOKEN=\"s3cret\"; $env:API_KEY = \"s3 cret\"; $env:PATH = \"C:\\bin\"",
        "$env:GH_TOKEN=***; $env:API_KEY = ***; $env:PATH = \"…\"",
    ),
    (
        "curl \"https://x/a?api_key=s1&page=2&access_token=s2&key=s3#top\"",
        "curl \"…\"",
    ),
    (
        "curl \"https://x/?sig=s1&X-Amz-Credential=s2&password=s3\"",
        "curl \"…\"",
    ),
    (
        "curl -H \"Cookie: sid=s3cret; a=b\" -H 'Set-Cookie: t=s3cret' x",
        "curl -H \"…\" -H '…' x",
    ),
    ("curl --cookie \"sid=s3cret\" x", "curl --cookie *** x"),
    (
        "psql postgresql://user:p@ss@db.example.com/app",
        "psql postgresql://***@db.example.com/app",
    ),
    ("--token=\"s3 cret\" x", "--token=*** x"),
    (
        "aws configure set aws_secret_access_key s3cret --profile p",
        "aws configure set aws_secret_access_key *** --profile p",
    ),
    (
        "npm config set //registry.npmjs.org/:_authToken s3cret",
        "npm config set //registry.npmjs.org/:_authToken ***",
    ),
    (
        "git config --global github.token s3cret",
        "git config --global github.token ***",
    ),
    (
        "echo \"//registry.npmjs.org/:_authToken=s3cret\" >> .npmrc",
        "echo \"…\" >> .npmrc",
    ),
    ("mysql -u root -ps3cret app", "mysql -u root -p*** app"),
    (
        "mysql -h db -P 3306 -ps3cret app",
        "mysql -h db -P 3306 -p*** app",
    ),
    (
        "mysqldump -h db -p s3cret app",
        "mysqldump -h db -p *** app",
    ),
    ("sshpass -p s3cret ssh host", "sshpass -p *** ssh host"),
    (
        "docker login -u me -p s3cret ghcr.io",
        "docker login -u me -p *** ghcr.io",
    ),
    (
        "echo npm_abcdefghijklmnopqrstuvwxyz0123 sk_live_abcdef123456 rk_live_abcdef123456",
        "echo *** *** ***",
    ),
    (
        "echo AIzaSyA-abcdefghijklmnopqrstuvwxyz012 ASIAABCDEFGHIJKLMNOP",
        "echo *** ***",
    ),
    (
        "echo \"-----BEGIN RSA PRIVATE KEY-----MIIEs3cret-----END RSA PRIVATE KEY-----\" > k",
        "echo \"…\" > k",
    ),
    (
        "echo \"-----BEGIN OPENSSH PRIVATE KEY----- s3cret\" > k",
        "echo \"…\" > k",
    ),
    (
        "curl -d @m.json https://hooks.slack.com/services/T00/B00/s3cret",
        "curl -d @m.json https://hooks.slack.com/services/***",
    ),
    (
        "curl -d {\\\"token\\\":\\\"s3cret\\\"} x",
        "curl -d {\\\"…\\\":\\\"…\\\"} x",
    ),
    (
        "python -c \"print({'password': 's3cret', 'pin_token': 1234})\"",
        "python -c \"…\"",
    ),
    (
        "OPENAI_KEY=s3 DB_PASS=s3 MYSQL_PWD=s3 X_PASSWD=s3 FOO_PW=s3 KEY=s3 MONKEY=ok BYPASS=ok KEY_ID=ok",
        "OPENAI_KEY=*** DB_PASS=*** MYSQL_PWD=*** X_PASSWD=*** FOO_PW=*** KEY=*** MONKEY=ok BYPASS=ok KEY_ID=ok",
    ),
    ("$env:OPENAI_KEY = \"s3cret\"", "$env:OPENAI_KEY = ***"),
    ("$env:GH_TOKEN = s3cret; echo", "$env:GH_TOKEN = ***; echo"),
    (
        "curl \"https://x/?db_pass=s3&X-Amz-Signature=s3&user.pwd=s3&x=1\"",
        "curl \"…\"",
    ),
    ("curl -d 'password=s3cret' x", "curl -d '…' x"),
    (
        "curl -d \"a=1&client_secret=s3cret&b=2\" x",
        "curl -d \"…\" x",
    ),
    (
        "kubectl create secret generic x --from-literal=password=s3cret",
        "kubectl create secret generic x --from-literal=password=***",
    ),
    (
        "helm install --set db.auth.rootPassword=s3cret x",
        "helm install --set db.auth.rootPassword=*** x",
    ),
    (
        "terraform apply -var=\"api_token=s3cret\"",
        "terraform apply -var=\"…\"",
    ),
    (
        "curl -HAuthorization:Bearer\\ s3cret x",
        "curl -HAuthorization:***",
    ),
    (
        "echo eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.s3cretsig",
        "echo ***",
    ),
    (
        "cargo login s3cret; cargo login --registry r s3cret && cargo publish",
        "cargo login ***; cargo login *** && cargo publish",
    ),
    (
        "openssl enc -aes256 -pass pass:s3cret -in f && openssl rsa -passin pass:s3cret",
        "openssl enc -aes256 -pass *** -in f && openssl rsa -passin pass:***",
    ),
    (
        "gh secret set NAME -b s3cret; gh secret set NAME --body=\"s3 cret\" -R o/r",
        "gh secret set NAME -b ***; gh secret set NAME --body=*** -R o/r",
    ),
    ("curl -H Cookie:sid=s3cret x", "curl -H Cookie:***"),
    (
        "curl https://x/a?api_key=s1\\&page=2\\&access_token=s2 x",
        "curl https://x/a?api_key=***&page=2\\&access_token=*** x",
    ),
    (
        "curl -d password=s3cret -d client_secret=s3cret x",
        "curl -d password=*** -d client_secret=*** x",
    ),
    (
        "terraform apply -var=api_token=s3cret",
        "terraform apply -var=api_token=***",
    ),
    (
        "echo //registry.npmjs.org/:_authToken=s3cret >> .npmrc",
        "echo //registry.npmjs.org/:_authToken=*** >> .npmrc",
    ),
    (
        "echo -----BEGIN RSA PRIVATE KEY-----MIIEs3cret > k",
        "echo ***",
    ),
    (
        "echo hf_abcdefghijklmnopqrstuvwxyz012345 pypi-AgEIcHlwaS5vcmcCJDAwMDAwMDAw glsa_abcdefghijklmnopqrstuvwxyz012345",
        "echo *** *** ***",
    ),
    (
        "echo dop_v1_0123456789abcdef0123456789abcdef SG.abcdefghijklmnop.qrstuvwxyz0123456789 shpat_0123456789abcdef0123456789abcdef",
        "echo *** *** ***",
    ),
    (
        "curl -b sid=s3cret x; curl --cookie sid=s3cret x; git checkout -b feature",
        "curl -b *** x; curl --cookie *** x; git checkout -b feature",
    ),
    (
        "app --pass s3cret --pw=s3cret --key s3cret --api-key s3cret --bypass ok --keep ok",
        "app --pass *** --pw=*** --key *** --api-key *** --bypass ok --keep ok",
    ),
    (
        "redis-cli -h db -a s3cret ping; az login -u me -p s3cret",
        "redis-cli -h db -a *** ping; az login -u me -p ***",
    ),
];

#[test]
fn the_forms_a_secret_takes_on_a_command_line() {
    let r = redactor();
    let table = SECRET_FORMS;

    check(&r, table);
    for &(cmd, _) in table {
        assert!(!r.redact(cmd).contains("s3"), "{cmd}");
    }
}

/// The quoted-string table of 1.x's test, each command with its redaction.
const QUOTED_STRINGS: &[(&str, &str)] = &[
    (
        "echo 'let api_key = \"s3cret\";' >> src/main.rs",
        "echo '…' >> src/main.rs",
    ),
    (
        "echo 'token = \"s3cret\"' >> Cargo.toml",
        "echo '…' >> Cargo.toml",
    ),
    (
        "printf \"const API_KEY: &str = \\\"s3cret\\\";\\n\" >> src/lib.rs",
        "printf \"…\" >> src/lib.rs",
    ),
    (
        "echo const API_KEY: \\&str = \\\"s3cret\\\"; >> src/lib.rs",
        "echo const API_KEY: ***\"…\\\"; >> src/lib.rs",
    ),
    (
        "echo $'key = \\'s3cret\\'' >> Cargo.toml",
        "echo $'…' >> Cargo.toml",
    ),
    (
        "sed -i \"s/old/s3cret/\" src/lib.rs",
        "sed -i \"…\" src/lib.rs",
    ),
    ("echo '' \"\" x", "echo '' \"\" x"),
    ("echo 's3cret >> src/lib.rs", "echo '…"),
    ("echo \"a \\\"s3cret\\\" b >> f", "echo \"…"),
    ("echo \\\"s3cret >> f", "echo \\\"…"),
    (
        "echo \"$(printf \"%s\" \"s3cret\")\" >> src/main.rs",
        "echo \"…",
    ),
    (
        "echo \"${X:-\"let k = hunter2xyz;\"}\" >> src/main.rs",
        "echo \"…",
    ),
    (
        "echo \"pub const K: &str = `echo \"s3cret\"`;\" >> src/lib.rs",
        "echo \"…",
    ),
    (
        "echo \"a \\$(b) \\`c\\`\" s3cret=ok",
        "echo \"…\" s3cret=ok",
    ),
    (
        "echo \\\"x \"let k=\\\"s3cret\\\";\" >> src/lib.rs",
        "echo \\\"…",
    ),
    (
        "echo \\\"x 'let k=\\\"s3cret\\\";' >> src/lib.rs",
        "echo \\\"…",
    ),
    ("echo \"it's\" s3cret=ok", "echo \"…\" s3cret=ok"),
];

#[test]
fn the_contents_of_quoted_strings_are_dropped_so_file_content_written_by_a_shell_never_reaches_telemetry()
 {
    let r = redactor();
    let table = QUOTED_STRINGS;

    check(&r, table);
    for &(cmd, _) in table {
        // 1.x: `!/s3cret(?!=ok)/.test(..)`.
        let out = r.redact(cmd);
        let leaked = out.match_indices("s3cret").any(|(at, _)| {
            !out.get(at..)
                .is_some_and(|rest| rest.starts_with("s3cret=ok"))
        });
        assert!(!leaked, "{cmd}");
    }
}

#[test]
fn a_megabyte_hyphenated_line_is_cut_and_a_cut_long_token_leaves_no_part() {
    let r = redactor();
    let long_line = format!("x {}\n{}", "a".repeat(1 << 20), "b-".repeat(500_000));
    let config = format!("config {}", "--a ".repeat(250_000));
    for big in [
        "a-".repeat(500_000),
        "token-".repeat(170_000),
        config,
        long_line,
    ] {
        let out = r.redact(&big);

        assert!(
            out.ends_with('…') && out.chars().count() <= 201,
            "{}",
            big.get(..12).unwrap_or(&big)
        );
    }
    // A first line cut back to its last whole word leaves no part of a long token.
    let cmd = format!("{}ghp_{}", "a ".repeat(1020), "x".repeat(40));
    assert_eq!(r.redact_to(&cmd, 5000), format!("{}…", "a ".repeat(1020)));
}

#[test]
fn clipped_to_200_characters_after_redaction_so_a_cut_secret_is_still_removed() {
    let r = redactor();
    let long = format!("{} GH_TOKEN={}", "x".repeat(180), "s".repeat(40));
    assert_eq!(r.redact(&long), format!("{} GH_TOKEN=***", "x".repeat(180)));
    assert_eq!(r.redact(&"y".repeat(300)), format!("{}…", "y".repeat(200)));
    assert_eq!(
        r.redact_to(&"y".repeat(300), 10),
        format!("{}…", "y".repeat(10))
    );
    assert_eq!(r.redact(""), "", "`redactCommand(undefined)`");
    // By code points: a character outside the BMP is not split into half a surrogate pair.
    assert_eq!(
        r.redact_to(&format!("{}\u{1F600}z", "y".repeat(9)), 10),
        format!("{}\u{1F600}…", "y".repeat(9))
    );
}

/// The patterns whose JS look-around or back-reference is rewritten (port plan 2-16), on inputs
/// where dropping the look-around gives another answer; each expected text is 1.x's.
#[test]
fn the_rewritten_look_around_patterns_redact_as_1x_does() {
    let r = redactor();
    check(
        &r,
        &[
            ("x -Htoken: s3 x", "x -Htoken: ***"),
            ("x-Htoken: s3", "x-Htoken: ***"),
            ("a -hcookie: s3", "a -hcookie: ***"),
            ("x \\'token\\': \\'s3 x\\' y", "x \\'token\\': \\'***\\' y"),
            ("x \\'token\\': \\'s3", "x \\'token\\': \\'s3"),
            ("x \\'pin_token\\': 1234, z", "x \\'pin_token\\': ***, z"),
            ("x \\'pin_token\\': \\1234", "x \\'pin_token\\': \\1234"),
            ("API_KEY=***x y", "API_KEY=***x y"),
            ("API_KEY=**x y", "API_KEY=*** y"),
            ("API_KEY=* y", "API_KEY=*** y"),
            ("API_KEY=** y", "API_KEY=*** y"),
            ("$env:API_KEY = ***x", "$env:API_KEY = ***x"),
            ("$env:API_KEY = *", "$env:API_KEY = ***"),
            (
                "authx=1 auth_x=s3 auth-x=s3 AUTH=s3 x",
                "authx=1 auth_x=*** auth-x=s3 AUTH=*** x",
            ),
            ("--authz s3 --auth s3", "--authz s3 --auth ***"),
            ("?authx=1&auth=s3", "?authx=1&auth=***"),
            (
                "config set authority s3 x; config set auth.x s3",
                "config set authority s3 x; config set auth.x ***",
            ),
            ("x \\'token\\': \\'ab'c\\' y", "x \\'token\\': \\'ab'…' y"),
            ("x \\'token\\': \\'ab\\'c y", "x \\'token\\': \\'***\\'c y"),
            ("x \\'token\\': 'ab' y", "x \\'token\\': '***' y"),
        ],
    );
}
