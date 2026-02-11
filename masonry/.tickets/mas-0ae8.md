---
id: mas-0ae8
status: closed
deps: [mas-47cf, mas-fa9d]
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 1
assignee: Bruce Mitchener
tags: [masonry, style, embedder]
---
# Embedder: derive SelectorInputs signature from WidgetState + Classes

Implement embedder-side helper to build (or signature-hash) selector inputs for a widget using: TypeTag (derived by embedder), Classes property, and pseudos from Masonry WidgetState (hover/active/focus/disabled). Disabled should behave inherited as in Masonry.

## Acceptance Criteria

- No per-frame allocations for pseudos/classes common case\n- Signature changes when hover/active/focus/disabled/classes change\n- Unit tests for ordering/dedup and signature stability

