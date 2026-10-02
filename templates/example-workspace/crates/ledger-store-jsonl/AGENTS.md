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
- An append is one batch: encoded in memory, written with one `write_all` at the committed length, then `sync_data`ed. If the write fails, the file is cut back to its previous length, so no reader sees part of a command's events.
- Bytes after the last newline are an append interrupted by a crash, never acknowledged: `load` skips them and the next `append` truncates them (`skips_and_then_repairs_an_append_interrupted_by_a_crash`).
- A `closed` event is written `{"type":"closed"}` and refuses any extra field: `serde` ignores `deny_unknown_fields` on unit variants, so the DTO variant wraps a unit struct, `Closed(ClosedV2)` (`closed_event_is_written_as_a_bare_type_tag_and_read_back`, `refuses_unknown_fields_even_on_field_less_events`).

## Errors
- `StoreError` (`#[non_exhaustive]`): `Read` / `Write` (I/O, with the path; a failed `Write` appended nothing), `PartialAppend` (the write failed and so did the rollback), `Corrupt` (path, line, and the `RecordError`), `RecordTooLong` (path, line, limit), `Conflict` (account, expected and actual versions). All are infrastructure failures from the application's point of view.
- `RecordError` (`#[non_exhaustive]`): why one line is not a valid record: malformed JSON or shape, missing or unsupported `schema_version`, or an owner name that fails domain validation.

## Test strategy
- Integration tests in `tests/it/` against a real temporary directory: round trip, empty history, conflict across two handles, crash recovery, v1 upcast, future schema, unknown fields, the `closed` tag, invalid owner, oversized line, and an `insta` snapshot of the written file.
- The format tests pin the `serde` behaviour the format relies on and that no signature states: field order, the bare `closed` tag, and refusal of unknown fields (TST-18).
- A crate-root doctest appends and loads one event in a temporary directory (TST-11).
- Under Miri the tests need `-Zmiri-disable-isolation` (temporary directories) and `INSTA_WORKSPACE_ROOT`, set by the CI job, not the code.
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
| 2026-10-02 | Stream files are opened read-write, not in append mode, and each batch is written at an explicit offset (`committed_len`) under the exclusive lock. On Windows an append-mode handle lacks `FILE_WRITE_DATA`, so `set_len` (torn-tail repair, failed-batch rollback) failed with `PermissionDenied`, which `skips_and_then_repairs_an_append_interrupted_by_a_crash` caught on Windows runners. | ARC-03, ERR-13 | example maintainers |
| 2026-09-28 | Stream files are named `account-<id>.jsonl`: the prefix keeps ids such as `con` clear of Windows reserved device names. | ARC-03 | example maintainers |
| 2026-09-28 | No directory `fsync` after creating a stream file: it is not portable, and a lost directory entry after a crash loses only the first append of a new account. | PERF-07 | example maintainers |
| 2026-09-28 | Add `serde`, `serde_json`, and dev-dependencies `tempfile` and `insta`. | H-16, ARC-12 | example maintainers |
| 2026-09-28 | The `closed` DTO variant wraps a unit struct (`Closed(ClosedV2)`) rather than being written `Closed {}`: the JSON is the same, `serde` still refuses extra fields, and no variant has empty braces. | OUT-02 | example maintainers |
| 2026-09-28 | Teardown needs no `close`/`finish`: each `append` calls `sync_data` on its success path and the `File` buffers nothing, so its `Drop` (close and unlock) cannot lose an acknowledged write. | ERR-13 | example maintainers |
| 2026-09-28 | `TryFrom<RecordV2>` consumes the DTO and does not hand it back on failure: the DTO exists only to be converted, and `decode` discards it either way. | ERR-14 | example maintainers |
| 2026-09-28 | `RecordError::Malformed` carries `serde_json::Error`, which makes `serde_json` part of this crate's API. Acceptable while the crate is workspace-internal; wrap it in an opaque error before publishing. | API-14, ERR-04 | example maintainers |
