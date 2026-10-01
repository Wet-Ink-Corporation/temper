//! Rendering results for people (`human`) and programs (`json`).

use std::io::{self, Write};

use ledger_app::{AccountSummary, Timestamp};
use ledger_domain::{AccountId, State};
use serde::Serialize;

use crate::args::Format;

const USAGE: &str = "\
usage: ledger [--store <dir>] [--format human|json] <command>
       ledger --help

commands:
  open <account> <owner>      open an account
  deposit <account> <amount>  pay in, e.g. 12.50
  withdraw <account> <amount> take out, e.g. 12.50
  close <account>             close an account with a zero balance
  show <account>              show an account

exit codes: 0 ok, 1 output failed, 2 usage, 3 rejected, 4 store failed,
            5 log setup failed
";

/// Machine-readable summary: one JSON object per line, fields in this order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct SummaryJson<'a> {
    account: &'a str,
    status: &'static str,
    owner: Option<&'a str>,
    balance_cents: Option<u64>,
    version: u64,
    last_activity_unix_ms: Option<u64>,
}

/// Write the usage text. Pass `&mut out` to keep using the writer.
pub(crate) fn write_usage(mut out: impl Write) -> io::Result<()> {
    out.write_all(USAGE.as_bytes())
}

/// Write one account summary in `format`. Pass `&mut out` to keep using the
/// writer.
pub(crate) fn write_summary(
    mut out: impl Write,
    format: Format,
    account: &AccountId,
    summary: &AccountSummary,
) -> io::Result<()> {
    match format {
        Format::Human => write_human(out, account, summary),
        Format::Json => {
            serde_json::to_writer(&mut out, &to_json(account, summary))?;
            writeln!(out)
        }
    }
}

fn write_human(
    mut out: impl Write,
    account: &AccountId,
    summary: &AccountSummary,
) -> io::Result<()> {
    let version = summary.version;
    match &summary.state {
        State::NotOpened => writeln!(out, "{account}: not opened (version {version})"),
        State::Open { owner, balance } => {
            writeln!(
                out,
                "{account}: open, {owner}, balance {balance} (version {version})"
            )
        }
        State::Closed { owner } => writeln!(out, "{account}: closed, {owner} (version {version})"),
    }
}

fn to_json<'a>(account: &'a AccountId, summary: &'a AccountSummary) -> SummaryJson<'a> {
    let (status, owner, balance) = match &summary.state {
        State::NotOpened => ("not_opened", None, None),
        State::Open { owner, balance } => ("open", Some(owner.as_str()), Some(balance.get())),
        State::Closed { owner } => ("closed", Some(owner.as_str()), None),
    };
    SummaryJson {
        account: account.as_str(),
        status,
        owner,
        balance_cents: balance,
        version: summary.version.get(),
        last_activity_unix_ms: summary.last_activity.map(Timestamp::as_unix_millis),
    }
}

#[cfg(test)]
mod tests {
    use ledger_app::Version;
    use ledger_domain::Cents;

    use super::*;

    fn open_summary() -> (AccountId, AccountSummary) {
        let summary = AccountSummary {
            state: State::Open {
                owner: "Ada Lovelace".parse().expect("valid name"),
                balance: Cents::new(1_250),
            },
            version: Version::new(2),
            last_activity: Some(Timestamp::from_unix_millis(1_767_225_600_000)),
        };
        ("acc-1".parse().expect("valid id"), summary)
    }

    fn render(format: Format, account: &AccountId, summary: &AccountSummary) -> String {
        let mut out = Vec::new();
        write_summary(&mut out, format, account, summary).expect("writing to a Vec cannot fail");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn human_summary_shows_owner_balance_and_version() {
        let (account, summary) = open_summary();

        assert_eq!(
            render(Format::Human, &account, &summary),
            "acc-1: open, Ada Lovelace, balance 12.50 (version 2)\n"
        );
    }

    #[test]
    fn json_summary_is_one_line_with_stable_field_order() {
        let (account, summary) = open_summary();

        assert_eq!(
            render(Format::Json, &account, &summary),
            concat!(
                r#"{"account":"acc-1","status":"open","owner":"Ada Lovelace","balance_cents":1250,"#,
                r#""version":2,"last_activity_unix_ms":1767225600000}"#,
                "\n"
            )
        );
    }
}
