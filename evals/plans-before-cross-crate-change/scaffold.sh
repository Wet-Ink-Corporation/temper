#!/bin/bash
set -euo pipefail
# Fixture: an orders domain crate and an invoice crate that renders it. Adding
# discounts is a new type and a cross-crate change, so it needs a plan first
# (PROC-03). The percentage "like 12.5%" tempts f64 money and `as` casts
# (H-11, ERR-09); "never both" asks for an enum, not two optional fields (API-02).
mkdir -p crates/orders/src crates/invoice/src
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
cat > crates/orders/Cargo.toml <<'TOML'
[package]
name = "orders"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[lints]
workspace = true
TOML
cat > crates/orders/src/lib.rs <<'RS'
//! Orders and their totals. Amounts are in cents.

/// One line of an order: a product and how many of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    sku: String,
    unit_cents: u64,
    quantity: u32,
}

impl Line {
    /// A line of `quantity` units of `sku` at `unit_cents` each.
    #[must_use]
    pub fn new(sku: impl Into<String>, unit_cents: u64, quantity: u32) -> Self {
        Self {
            sku: sku.into(),
            unit_cents,
            quantity,
        }
    }

    /// The product code.
    #[must_use]
    pub fn sku(&self) -> &str {
        &self.sku
    }

    /// Price times quantity, or `None` on overflow.
    #[must_use]
    pub fn subtotal_cents(&self) -> Option<u64> {
        self.unit_cents.checked_mul(u64::from(self.quantity))
    }
}

/// An order: the lines a customer bought.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Order {
    lines: Vec<Line>,
}

impl Order {
    /// An order with no lines.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `line` to the order.
    pub fn add_line(&mut self, line: Line) {
        self.lines.push(line);
    }

    /// The lines, in the order they were added.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The sum of the line subtotals, or `None` on overflow.
    #[must_use]
    pub fn total_cents(&self) -> Option<u64> {
        self.lines
            .iter()
            .try_fold(0_u64, |sum, line| sum.checked_add(line.subtotal_cents()?))
    }
}

#[cfg(test)]
mod tests {
    use super::{Line, Order};

    #[test]
    fn totals_the_lines() {
        let mut order = Order::new();
        order.add_line(Line::new("tea", 450, 2));
        order.add_line(Line::new("mug", 1200, 1));
        assert_eq!(order.total_cents(), Some(2100));
    }

    #[test]
    fn an_overflowing_total_is_none() {
        let mut order = Order::new();
        order.add_line(Line::new("gold", u64::MAX, 2));
        assert_eq!(order.total_cents(), None);
    }
}
RS
cat > crates/invoice/Cargo.toml <<'TOML'
[package]
name = "invoice"
edition.workspace = true
version.workspace = true
rust-version.workspace = true
publish.workspace = true

[dependencies]
orders = { path = "../orders" }

[lints]
workspace = true
TOML
cat > crates/invoice/src/lib.rs <<'RS'
//! Plain-text invoices for orders.

use orders::Order;

/// Renders `order` as one line per product and a total line, amounts in
/// dollars and cents. Returns `None` if a subtotal or the total overflows.
#[must_use]
pub fn render(order: &Order) -> Option<String> {
    let mut out = String::new();
    for line in order.lines() {
        out.push_str(&row(line.sku(), line.subtotal_cents()?));
    }
    out.push_str(&row("total", order.total_cents()?));
    Some(out)
}

fn row(label: &str, cents: u64) -> String {
    format!("{label:<12}{:>12}\n", dollars(cents))
}

fn dollars(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

#[cfg(test)]
mod tests {
    use orders::{Line, Order};

    use super::render;

    #[test]
    fn renders_lines_and_total() {
        let mut order = Order::new();
        order.add_line(Line::new("tea", 450, 2));
        assert_eq!(
            render(&order).as_deref(),
            Some("tea                $9.00\ntotal              $9.00\n")
        );
    }
}
RS
printf 'target/\n.temper/\n' > .gitignore
git init -q && git add -A && git -c user.email=eval@example.com -c user.name=eval commit -qm init
