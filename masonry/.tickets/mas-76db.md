---
id: mas-76db
status: open
deps: []
links: [mas-fa9d]
created: 2026-02-11T03:29:34Z
type: task
priority: 3
assignee: Bruce Mitchener
tags: [understory, style]
---
# Upstream Understory: faster pseudos/classes representation (optional)

Consider adding helper APIs to understory_style to avoid per-pass sorting/alloc for pseudos/classes, e.g. bitset-backed pseudos fast-path or a signature/hashing utility. Track as upstream improvement; may be external-ref'd later.

## Acceptance Criteria

- Proposed API sketched\n- Tradeoffs documented\n- Decision: implement now vs defer

