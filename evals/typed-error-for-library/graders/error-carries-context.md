---
type: llm
focus: { source: file, path: crates/endpoint/src/lib.rs }
---

PASS if the error type that `parse_endpoint` returns has separate variants (or an equivalent typed kind) for at least two different failures, such as a missing `:` separator, an empty host, or a port that is not a number from 1 to 65535, and the variant for a bad port keeps the offending input as a typed field (for example the original text, or the `ParseIntError` as its source) rather than only a preformatted message string. The type implements `std::fmt::Display` and `std::error::Error`.
FAIL if the error is a `String`, `&str` or `Box<dyn Error>`, if it is a single variant or struct holding only a formatted message, or if it does not implement `std::error::Error`.
