---
id: mas-870f
status: open
deps: [mas-fa9d, mas-47cf]
links: []
created: 2026-02-11T03:37:46Z
type: task
priority: 2
assignee: Bruce Mitchener
tags: [masonry, style, migration]
---
# Plan: deprecate duplicated-state properties via style pseudos

Write a concrete migration plan to reduce/remove duplicated-state properties (e.g. ActiveBackground/DisabledBackground/HoveredBorderColor/FocusedBorderColor) once pseudo/class-based styling is in place. Include compatibility strategy (bridge period), deprecation notes, and criteria for removal.

## Acceptance Criteria

- List of candidate properties + target replacements\n- Proposed bridging strategy + timeline\n- Decide which removals are breaking and how to stage them

