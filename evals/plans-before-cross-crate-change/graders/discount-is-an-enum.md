---
type: regex
target: { source: file, path: crates/orders/src/discount.rs }
# API-02: "one or the other, never both" is an enum, not two optional fields.
pattern: 'pub enum \w*Discount\w*\s*(?:<[^>]*>)?\s*\{'
---
