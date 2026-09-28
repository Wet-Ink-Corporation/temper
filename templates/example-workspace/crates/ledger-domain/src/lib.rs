//! Ledger domain: bank accounts as event-sourced deciders.
//!
//! Pure business rules with no I/O, no clock and no serialization. An
//! account's state is a fold over its events; [`decide`] says which events a
//! command produces, and [`evolve`] applies one event to the state.

mod account;
mod account_id;
mod money;
mod owner_name;

pub use account::{Command, Event, Rejection, State, decide, evolve, fold};
pub use account_id::{AccountId, AccountIdError};
pub use money::{Cents, ParseCentsError};
pub use owner_name::{OwnerName, OwnerNameError};
