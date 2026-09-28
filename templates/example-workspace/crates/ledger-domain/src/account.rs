//! The account aggregate in Decider shape: commands, events, state, and the
//! pure functions between them.

use thiserror::Error;

use crate::money::Cents;
use crate::owner_name::OwnerName;

/// An intent to change an account. May be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open the account for its holder.
    Open {
        /// Who holds the account.
        owner: OwnerName,
    },
    /// Pay money in.
    Deposit {
        /// How much to pay in; must not be zero.
        amount: Cents,
    },
    /// Take money out.
    Withdraw {
        /// How much to take out; must not be zero or exceed the balance.
        amount: Cents,
    },
    /// Close the account. Its balance must be zero.
    Close,
}

/// A fact about an account. Past tense and immutable: once recorded, an event
/// is never edited or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The account was opened.
    Opened {
        /// Who holds the account.
        owner: OwnerName,
    },
    /// Money was paid in.
    Deposited {
        /// How much was paid in.
        amount: Cents,
    },
    /// Money was taken out.
    Withdrew {
        /// How much was taken out.
        amount: Cents,
    },
    /// The account was closed.
    Closed,
}

/// What [`decide`] needs to know about an account, rebuilt by [`fold`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum State {
    /// No `Opened` event yet.
    #[default]
    NotOpened,
    /// Open for business.
    Open {
        /// Who holds the account.
        owner: OwnerName,
        /// Current balance.
        balance: Cents,
    },
    /// Closed for good; closed accounts never reopen.
    Closed {
        /// Who held the account.
        owner: OwnerName,
    },
}

/// Why a command was refused. Business outcomes, never infrastructure
/// failures: retrying the same command against the same state changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum Rejection {
    /// `Open` on an account that is already open.
    #[error("account is already open")]
    AlreadyOpen,
    /// A command other than `Open` on an account that was never opened.
    #[error("account has not been opened")]
    NotOpened,
    /// Any command on a closed account.
    #[error("account is closed")]
    AccountClosed,
    /// A deposit or withdrawal of zero.
    #[error("amount must be greater than zero")]
    ZeroAmount,
    /// A withdrawal larger than the balance.
    #[error("insufficient funds: balance {balance}, requested {requested}")]
    InsufficientFunds {
        /// Balance at the time of the request.
        balance: Cents,
        /// Amount requested.
        requested: Cents,
    },
    /// A deposit that would take the balance past the largest representable amount.
    #[error("deposit of {deposit} would overflow the balance {balance}")]
    BalanceOverflow {
        /// Balance at the time of the request.
        balance: Cents,
        /// Amount deposited.
        deposit: Cents,
    },
    /// `Close` on an account that still holds money.
    #[error("cannot close an account holding {balance}")]
    BalanceNotZero {
        /// Remaining balance.
        balance: Cents,
    },
}

/// Decide which events `command` produces against `state`.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use ledger_domain::{Cents, Command, Event, Rejection, State, decide, fold};
///
/// let history = [
///     Event::Opened { owner: "Ada Lovelace".parse()? },
///     Event::Deposited { amount: Cents::new(500) },
/// ];
/// let overdraw = Command::Withdraw { amount: Cents::new(501) };
///
/// assert_eq!(
///     decide(&overdraw, &fold(&history)),
///     Err(Rejection::InsufficientFunds { balance: Cents::new(500), requested: Cents::new(501) }),
/// );
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns a [`Rejection`] when the command would break an account rule:
/// opening twice, moving money on an account that is not open, a zero amount,
/// overdrawing, overflowing the balance, or closing with money left.
pub fn decide(command: &Command, state: &State) -> Result<Vec<Event>, Rejection> {
    match (command, state) {
        (Command::Open { owner }, State::NotOpened) => {
            // Clone: the event owns its data and outlives the borrowed command.
            Ok(vec![Event::Opened {
                owner: owner.clone(),
            }])
        }
        (Command::Open { .. }, State::Open { .. }) => Err(Rejection::AlreadyOpen),
        (Command::Deposit { .. } | Command::Withdraw { .. } | Command::Close, State::NotOpened) => {
            Err(Rejection::NotOpened)
        }
        (
            Command::Open { .. }
            | Command::Deposit { .. }
            | Command::Withdraw { .. }
            | Command::Close,
            State::Closed { .. },
        ) => Err(Rejection::AccountClosed),
        (Command::Deposit { amount }, State::Open { balance, .. }) => deposit(*balance, *amount),
        (Command::Withdraw { amount }, State::Open { balance, .. }) => withdraw(*balance, *amount),
        (Command::Close, State::Open { balance, .. }) => close(*balance),
    }
}

fn deposit(balance: Cents, amount: Cents) -> Result<Vec<Event>, Rejection> {
    if amount.is_zero() {
        return Err(Rejection::ZeroAmount);
    }
    balance
        .checked_add(amount)
        .ok_or(Rejection::BalanceOverflow {
            balance,
            deposit: amount,
        })?;
    Ok(vec![Event::Deposited { amount }])
}

fn withdraw(balance: Cents, amount: Cents) -> Result<Vec<Event>, Rejection> {
    if amount.is_zero() {
        return Err(Rejection::ZeroAmount);
    }
    balance
        .checked_sub(amount)
        .ok_or(Rejection::InsufficientFunds {
            balance,
            requested: amount,
        })?;
    Ok(vec![Event::Withdrew { amount }])
}

fn close(balance: Cents) -> Result<Vec<Event>, Rejection> {
    if balance.is_zero() {
        Ok(vec![Event::Closed])
    } else {
        Err(Rejection::BalanceNotZero { balance })
    }
}

/// Apply one event to the state.
///
/// Total and infallible: events are facts that [`decide`] already checked, so
/// replaying history can never fail. An event that [`decide`] could not have
/// produced in this state leaves the state unchanged.
#[must_use]
pub fn evolve(state: State, event: &Event) -> State {
    match (state, event) {
        (State::NotOpened, Event::Opened { owner }) => State::Open {
            // Clone: the state owns its data; the event stays in the history.
            owner: owner.clone(),
            balance: Cents::ZERO,
        },
        (State::Open { owner, balance }, Event::Deposited { amount }) => State::Open {
            owner,
            balance: balance.checked_add(*amount).unwrap_or(balance),
        },
        (State::Open { owner, balance }, Event::Withdrew { amount }) => State::Open {
            owner,
            balance: balance.checked_sub(*amount).unwrap_or(balance),
        },
        (State::Open { owner, .. }, Event::Closed) => State::Closed { owner },
        (state @ (State::NotOpened | State::Open { .. } | State::Closed { .. }), _) => state,
    }
}

/// Rebuild an account's state from its history, oldest event first.
#[must_use]
pub fn fold<'history>(history: impl IntoIterator<Item = &'history Event>) -> State {
    history.into_iter().fold(State::NotOpened, evolve)
}

// Compile-time proof that these types can cross threads (ASY-11).
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Command>();
    assert_send_sync::<Event>();
    assert_send_sync::<State>();
    assert_send_sync::<Rejection>();
};

#[cfg(test)]
mod tests;
