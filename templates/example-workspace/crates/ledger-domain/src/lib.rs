//! Ledger domain: bank accounts as event-sourced deciders.
//!
//! Pure business rules with no I/O, no clock and no serialization. An
//! account's state is a fold over its events; [`decide`] says which events a
//! command produces, and [`evolve`] applies one event to the state. Values
//! enter through [`FromStr`](std::str::FromStr): [`AccountId`], [`OwnerName`]
//! and [`Cents`] cannot hold invalid text.
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use ledger_domain::{Cents, Command, State, decide, evolve};
//!
//! let commands = [
//!     Command::Open { owner: "Ada Lovelace".parse()? },
//!     Command::Deposit { amount: "12.50".parse()? },
//! ];
//! let mut state = State::NotOpened;
//! for command in &commands {
//!     for event in decide(command, &state)? {
//!         state = evolve(state, &event);
//!     }
//! }
//!
//! assert_eq!(
//!     state,
//!     State::Open { owner: "Ada Lovelace".parse()?, balance: Cents::new(1_250) },
//! );
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! This crate has no Cargo features.

mod account;
mod account_id;
mod money;
mod owner_name;

pub use account::{Command, Event, Rejection, State, decide, evolve, fold};
pub use account_id::{AccountId, ParseAccountIdError};
pub use money::{Cents, ParseCentsError};
pub use owner_name::{OwnerName, ParseOwnerNameError};
