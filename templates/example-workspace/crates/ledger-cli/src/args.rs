//! Command-line arguments, parsed once into validated domain values.

use std::ffi::OsString;
use std::path::PathBuf;

use ledger_domain::{AccountId, AccountIdError, Cents, Command, OwnerNameError, ParseCentsError};
use thiserror::Error;

/// Where streams live when `--store` is not given.
const DEFAULT_STORE: &str = "ledger-data";

/// A fully parsed command line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub(crate) store: PathBuf,
    pub(crate) format: Format,
    pub(crate) request: Request,
}

/// What the user asked for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Help,
    Show(AccountId),
    Execute(AccountId, Command),
}

/// How results are written to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Human,
    Json,
}

/// Why the command line could not be parsed.
#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum UsageError {
    #[error("argument {} is not valid UTF-8", .0.display())]
    NotUnicode(OsString),
    #[error("{flag} needs a value")]
    MissingValue { flag: &'static str },
    #[error("unknown option {0:?}")]
    UnknownOption(String),
    #[error("unknown format {0:?}; expected human or json")]
    UnknownFormat(String),
    #[error("missing command")]
    MissingCommand,
    #[error("unknown command {0:?}")]
    UnknownCommand(String),
    #[error("wrong number of arguments for {0:?}")]
    WrongArity(String),
    #[error("invalid account id")]
    Account(#[source] AccountIdError),
    #[error("invalid amount")]
    Amount(#[source] ParseCentsError),
    #[error("invalid owner name")]
    Owner(#[source] OwnerNameError),
}

/// Parse the arguments after the program name.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, UsageError> {
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.into_string().map_err(UsageError::NotUnicode))
        .collect::<Result<_, _>>()?;
    let mut store = PathBuf::from(DEFAULT_STORE);
    let mut format = Format::Human;
    let mut words = Vec::with_capacity(args.len());
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        match arg {
            "-h" | "--help" => words = vec!["help"],
            "--store" => {
                store = PathBuf::from(
                    args.next()
                        .ok_or(UsageError::MissingValue { flag: "--store" })?,
                );
            }
            "--format" => {
                format = match args.next() {
                    Some("human") => Format::Human,
                    Some("json") => Format::Json,
                    Some(other) => return Err(UsageError::UnknownFormat(other.to_owned())),
                    None => return Err(UsageError::MissingValue { flag: "--format" }),
                };
            }
            option if option.starts_with("--") => {
                return Err(UsageError::UnknownOption(option.to_owned()));
            }
            word => words.push(word),
        }
    }
    Ok(Invocation {
        store,
        format,
        request: parse_request(&words)?,
    })
}

fn parse_request(words: &[&str]) -> Result<Request, UsageError> {
    let account = |text: &str| text.parse().map_err(UsageError::Account);
    let amount = |text: &str| text.parse::<Cents>().map_err(UsageError::Amount);
    let execute = |id: &str, command| Ok(Request::Execute(account(id)?, command));
    match *words {
        [] => Err(UsageError::MissingCommand),
        ["help", ..] => Ok(Request::Help),
        ["show", id] => Ok(Request::Show(account(id)?)),
        ["open", id, owner] => {
            let owner = owner.parse().map_err(UsageError::Owner)?;
            execute(id, Command::Open { owner })
        }
        ["deposit", id, cents] => execute(
            id,
            Command::Deposit {
                amount: amount(cents)?,
            },
        ),
        ["withdraw", id, cents] => execute(
            id,
            Command::Withdraw {
                amount: amount(cents)?,
            },
        ),
        ["close", id] => execute(id, Command::Close),
        [
            command @ ("show" | "open" | "deposit" | "withdraw" | "close"),
            ..,
        ] => Err(UsageError::WrongArity(command.to_owned())),
        [other, ..] => Err(UsageError::UnknownCommand(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    fn parse_words(words: &[&str]) -> Result<Invocation, UsageError> {
        parse(words.iter().map(OsString::from))
    }

    #[test]
    fn parses_options_before_the_command() {
        let invocation = parse_words(&[
            "--store", "/tmp/l", "--format", "json", "deposit", "acc-1", "12.5",
        ]);

        assert_eq!(
            invocation,
            Ok(Invocation {
                store: PathBuf::from("/tmp/l"),
                format: Format::Json,
                request: Request::Execute(
                    "acc-1".parse().expect("valid id"),
                    Command::Deposit {
                        amount: Cents::new(1_250)
                    },
                ),
            })
        );
    }

    #[test]
    fn defaults_to_human_output_and_local_store() {
        let invocation = parse_words(&["show", "acc-1"]).expect("valid arguments");

        assert_eq!(
            (invocation.store, invocation.format),
            (PathBuf::from(DEFAULT_STORE), Format::Human)
        );
    }

    #[test]
    fn open_takes_the_owner_as_one_argument() {
        let request = parse_words(&["open", "acc-1", "Ada Lovelace"]).map(|i| i.request);

        assert_eq!(
            request,
            Ok(Request::Execute(
                "acc-1".parse().expect("valid id"),
                Command::Open {
                    owner: "Ada Lovelace".parse().expect("valid name")
                },
            ))
        );
    }

    #[test]
    fn rejects_negative_amount_as_invalid() {
        let result = parse_words(&["withdraw", "acc-1", "-5"]);

        assert_matches!(
            result,
            Err(UsageError::Amount(ParseCentsError::Malformed { .. }))
        );
    }

    #[test]
    fn rejects_missing_argument_for_known_command() {
        assert_eq!(
            parse_words(&["close"]),
            Err(UsageError::WrongArity("close".to_owned()))
        );
    }

    #[test]
    fn rejects_unknown_format() {
        assert_eq!(
            parse_words(&["--format", "xml", "show", "acc-1"]),
            Err(UsageError::UnknownFormat("xml".to_owned()))
        );
    }
}
