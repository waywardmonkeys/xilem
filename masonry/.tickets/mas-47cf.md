---
id: mas-47cf
status: open
deps: []
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 1
assignee: Bruce Mitchener
tags: [masonry, style, design]
---
# Design: style integration boundary + caching

Decide: where selector inputs/signatures live, what gets cached (bundles vs per-property), and how invalidation channels (LAYOUT/PAINT/TEXT) map to style changes. Also decide whether/when to consume understory_* crates directly vs embedder-only glue.

## Acceptance Criteria

- Written design note (ticket notes ok) covering: signature, cache tiers, invalidation\n- Explicit decision on dependency strategy

