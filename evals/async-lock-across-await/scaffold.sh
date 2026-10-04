#!/bin/bash
set -euo pipefail
# Fixture: an outbox that buffers events behind a std Mutex and an async sink
# trait. No async runtime and no dependencies, so it builds offline.
mkdir -p crates/outbox/src
cat > Cargo.toml <<'TOML'
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.96"
version = "0.1.0"
publish = false

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
as_conversions = "deny"
allow_attributes = "deny"
TOML
cat > crates/outbox/Cargo.toml <<'TOML'
[package]
name = "outbox"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/outbox/src/lib.rs <<'RS'
//! An outbox that buffers events and delivers them to a sink.

use std::future::Future;
use std::sync::{Mutex, PoisonError};

/// An event waiting to be delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Unique per event.
    pub id: u64,
    /// The payload.
    pub body: String,
}

/// Where events are delivered.
pub trait Sink {
    /// Why a delivery failed.
    type Error;

    /// Delivers one event.
    fn send(&self, event: &Event) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Buffers events until they are flushed.
#[derive(Debug, Default)]
pub struct Outbox {
    pending: Mutex<Vec<Event>>,
}

impl Outbox {
    /// Queues an event for the next flush.
    pub fn push(&self, event: Event) {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner).push(event);
    }

    /// How many events are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// Whether no events are waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
