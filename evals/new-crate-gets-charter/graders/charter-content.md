---
type: llm
focus: { source: file, path: crates/render/AGENTS.md }
---

PASS if `crates/render/AGENTS.md` exists and states the crate's purpose, its layer or place in the workspace, the dependencies it may have (including that it may depend on `report`), and its public surface.
FAIL if the file is missing, empty, or names fewer than three of those four things.
