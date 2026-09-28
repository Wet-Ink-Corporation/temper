---
paths:
  - "**/*.rs"
---

# Rust source rules (rust-craft)

This workspace follows the rust-craft standard. The full standard is the `temper:rust-craft` skill; load it before writing Rust. Every rule has an ID, and reviewers cite those IDs.

Iron rules for production code. Tests may use `unwrap`, `expect` and `assert!`.
- H-01 No `unsafe`. H-02 No `unwrap`/`expect`: use `?`, `ok_or_else` or `let ... else`. H-03 No `panic!`/`todo!`/`unimplemented!`/`unreachable!`: encode invariants in types. H-04 No panicking indexing: use `get`, iterators or slice patterns.
- H-05 No `#[allow]`. `#[expect(lint, reason = "...")]` needs human approval. Never edit lint, CI or hook config to get green.
- H-06 No `dbg!`, no print macros (libraries use `tracing`, binaries use `writeln!` on a locked handle), no commented-out code, and no TODO without an issue link.
- H-07 No `Box::leak`, `mem::forget`, `transmute`, `static mut`, or `'static` bounds to silence lifetimes. H-08 Every `.clone()` has an ownership reason.
- H-09 Library APIs return typed `thiserror` errors, never `String`, `Box<dyn Error>` or `anyhow`. H-10 Never discard a failure (`let _ =`, `.ok();`). H-11 No lossy `as` casts: use `From`/`TryFrom`.
- H-12 Never weaken tests or checks to pass. H-13 Channels, buffers, tasks and I/O are bounded and owned. H-14 No blocking or held locks across `.await`. H-15 Zero warnings; public items documented, with `# Errors`.
- H-16 A new dependency, an API break, a lint exception, a crate-boundary change, a migration or a test rewrite is a human decision. Ask first.

Signatures borrow (`&str`, `&[T]`, `&Path`, `impl AsRef<_>`) unless the function stores the value (OWN-01). A validated value is a newtype with a `TryFrom` constructor (API-03). Keep the surface minimal: private, then `pub(crate)`, then deliberate `pub` (API-14).

Workflow: plan under `.temper/plans/`, then red, green, refactor, self-review, the gate (`cargo fmt`, `clippy -D warnings`, tests, `rustdoc -D warnings`), and a `temper:rust-reviewer` verdict. End with the Status block.
