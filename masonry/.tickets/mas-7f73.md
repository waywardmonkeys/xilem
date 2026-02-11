---
id: mas-7f73
status: open
deps: [mas-47cf, mas-fa9d]
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 1
assignee: Bruce Mitchener
tags: [masonry, style, embedder, text]
---
# Embedder: ComputedText bundle + TEXT/PAINT invalidation

Define a shared ComputedText bundle (font-ish props, line breaking, foreground, etc.) and resolution/caching strategy. Split invalidation between TEXT (relayout) and PAINT (recolor).

## Acceptance Criteria

- Bundle type and resolver implemented\n- Distinguish TEXT vs PAINT-affecting props\n- Unit tests for precedence and dirties

