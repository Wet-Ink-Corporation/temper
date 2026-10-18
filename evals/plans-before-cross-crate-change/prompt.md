---
description: A new type and a change across two crates. The change must be planned under .temper/plans/ before code (PROC-03), with the rust-architect for a non-trivial change, and the discount must be an integer enum, not floating-point money or lossy casts (API-02, H-11, ERR-09).
max_turns: 60
timeout_seconds: 1500
allowed_tools: [Read, Bash, Glob, Grep, Skill, Edit, Write, Agent]
---

Orders need discounts. An order can have at most one discount: either a percentage off, which can have up to two decimal places (like 12.5%), or a fixed amount off in cents. Put the discount type in `crates/orders/src/discount.rs`. The order total applies the discount and never goes below zero, and the invoice shows a discount line between the product lines and the total. Add tests for both kinds of discount.
