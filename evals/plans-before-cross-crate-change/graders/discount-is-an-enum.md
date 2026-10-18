---
type: regex
target: { source: file, path: crates/orders/src/discount.rs }
# API-02: "one or the other, never both" is an enum, not two optional fields.
# The enum may be private behind a public wrapper (`pub struct Discount(DiscountKind)`).
pattern: '\benum\s+\w*Discount\w*\s*(?:<[^>]*>)?\s*\{'
---
