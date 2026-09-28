# Crate charter: `ledger-store-jsonl`

## Purpose
Implements the `EventStore` port with append-only JSON Lines files: `<root>/account-<account-id>.jsonl`, one versioned record per line. It owns the persisted record format, its versions and its upcasters. Its own crate because it is the only place that knows about files and JSON (ARC-07).

## Layer
adapter (file system, JSON Lines).

## Allowed dependencies
- Workspace crates: `ledger-app` (the port), `ledger-domain` (the events it maps).
- Third-party: `serde` and `serde_json` (the record format), `thiserror` (`StoreError`, `RecordError`). Dev only: `tempfile` (per-test directories, TST-10), `insta` (format snapshot, TST-07).
- Never: other adapters, `tracing-subscriber`, `anyhow`.

## Public surface
Re-exported from `lib.rs`: `JsonlStore` (`new`, plus the `EventStore` impl), `StoreError`, `RecordError`. The DTOs (`RecordV2`, `EventV2`, `RecordV1`) are private. Workspace-internal, not published.

## Invariants
- Only the current schema (v2) is written; v1 records are upcast on read, and newer versions are refused (`upcasts_schema_v1_records_on_read`, `refuses_records_from_a_newer_schema`).
- Every stored field is validated into domain types on read; nothing deserialized is trusted (`validates_stored_owner_names_into_the_domain`).
- Output is deterministic: fixed field order, no maps, time supplied by the caller (`writes_one_current_schema_record_per_line` snapshot).
- A line longer than 65,535 bytes is refused before it is parsed, so one damaged line cannot claim unbounded memory (`refuses_a_line_longer_than_the_record_limit`). The number of lines is not capped: a stream is the account's whole history and is loaded whole.
- An append from a stale version gets `Conflict` and writes nothing (`append_from_stale_version_is_refused_and_writes_nothing`). The check and the write happen under an exclusive advisory lock (`File::lock`), which is what makes this hold across processes; the tests exercise the check, not the lock.
- An append is one batch: encoded in memory, written with one `write_all`, then `sync_data`ed. If the write fails, the file is cut back to its previous length, so no reader sees part of a command's events.
- Bytes after the last newline are an append interrupted by a crash, never acknowledged: `load` skips them and the next `append` truncates them (`skips_and_then_repairs_an_append_interrupted_by_a_crash`).
- `serde` only enforces `deny_unknown_fields` on struct variants, so the field-less `Closed` DTO variant is written `Closed {}` (`refuses_unknown_fields_even_on_field_less_events`).

## Errors
- `StoreError` (`#[non_exhaustive]`): `Read` / `Write` (I/O, with the path; a failed `Write` appended nothing), `PartialAppend` (the write failed and so did the rollback), `Corrupt` (path, line, and the `RecordError`), `RecordTooLong` (path, line, limit), `Conflict` (account, expected and actual versions). All are infrastructure failures from the application's point of view.
- `RecordError` (`#[non_exhaustive]`): why one line is not a valid record: malformed JSON or shape, missing or unsupported `schema_version`, or an owner name that fails domain validation.

## Test strategy
- Integration tests in `tests/it/` against a real temporary directory: round trip, empty history, conflict across two handles, crash recovery, v1 upcast, future schema, unknown fields, invalid owner, oversized line, and an `insta` snapshot of the written file.
- Run: `cargo test -p ledger-store-jsonl`; review snapshot changes with `cargo insta review` (never accept blindly, H-12).

## Do not
- Do not edit `src/record/v1.rs`: it describes bytes already on disk. Upcasters live in `src/record.rs`. A new shape is a new frozen version module plus an upcaster, and a schema change is a human decision (H-16, OUT-03).
- Do not derive `Deserialize` on domain types to skip the DTOs (OUT-05).
- Do not read without the line bound, or write without the lock.
- Do not write a record without its trailing newline; the newline is the commit marker.

## Decisions
| Date | Decision | Rule | Decided by |
|---|---|---|---|
| 2026-09-28 | Record format v2: `{"schema_version":2,"recorded_at_ms":…,"event":{"type":…}}`, event internally tagged by `type`. v1 (flat, tagged by `kind`, `withdrawn`) is upcast on read. | OUT-02, OUT-03 | example maintainers |
| 2026-09-28 | Read the version tag by parsing the line into a JSON map once, removing `schema_version`, then converting the rest into the versioned shape with `deny_unknown_fields`. | OUT-02, OUT-05 | example maintainers |
| 2026-09-28 | An append re-reads the whole stream to find the current version. Simple and correct under the lock; streams are small. Add an index or cached version only with a benchmark (PERF-01). | PERF-01 | example maintainers |
| 2026-09-28 | Stream files are named `account-<id>.jsonl`: the prefix keeps ids such as `con` clear of Windows reserved device names. | ARC-03 | example maintainers |
| 2026-09-28 | No directory `fsync` after creating a stream file: it is not portable, and a lost directory entry after a crash loses only the first append of a new account. | PERF-07 | example maintainers |
| 2026-09-28 | Add `serde`, `serde_json`, and dev-dependencies `tempfile` and `insta`. | H-16, ARC-12 | example maintainers |
