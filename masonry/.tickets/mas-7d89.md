---
id: mas-7d89
status: open
deps: [mas-47cf, mas-fa9d]
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 1
assignee: Bruce Mitchener
tags: [masonry, style, embedder]
---
# Embedder: ComputedBox bundle + LAYOUT/PAINT invalidation

Define a shared ComputedBox bundle (padding/border/background/border_color/corner_radius/box_shadow) computed from (local/animation overrides + style cascade + theme + inherited/default) and cached per element by signature/epochs. Use LAYOUT vs PAINT invalidation appropriately.

## Acceptance Criteria

- Bundle type and resolver implemented\n- Cache keyed by signature + theme/cascade epochs\n- Unit tests for precedence and channel dirties

