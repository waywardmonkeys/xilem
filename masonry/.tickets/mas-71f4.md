---
id: mas-71f4
status: open
deps: [mas-47cf, mas-fa9d, mas-7d89]
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 2
assignee: Bruce Mitchener
tags: [masonry, style, prototype]
---
# Prototype: Button styling via pseudos/classes

Prototype converting Masonry Button from duplicated state properties (ActiveBackground/DisabledBackground/HoveredBorderColor/FocusedBorderColor) to single-slot properties driven by pseudo rules in style layer. Keep behavior parity.

## Acceptance Criteria

- Button visual state transitions preserved\n- Snapshot tests updated as needed\n- No new per-widget computed struct

