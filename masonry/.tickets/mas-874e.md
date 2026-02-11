---
id: mas-874e
status: open
deps: [mas-47cf, mas-fa9d, mas-7f73]
links: []
created: 2026-02-11T03:29:34Z
type: task
priority: 2
assignee: Bruce Mitchener
tags: [masonry, style, prototype, text]
---
# Prototype: Label/text styling via ComputedText

Prototype wiring Label (or a minimal text widget) to consume ComputedText foreground/linebreaking/etc with :disabled pseudo rather than parallel DisabledContentColor-style duplication.

## Acceptance Criteria

- Disabled text styling preserved\n- TEXT vs PAINT invalidation exercised\n- Tests updated

