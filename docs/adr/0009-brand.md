# ADR-0009: Brand and Theming

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck needs a brand identity and a multi-theme system driven by design tokens (no raw colors in components).

## Decision

### Brand Colors

- **Primary**: Deep blue `#1B4D7E` (RGB 27, 77, 126)
- **Accent**: Teal `#2DD4BF` (RGB 45, 212, 191)

### Logo

Geometric line style: rounded `D` outline with three connected modules representing Skill, Prompt, and MCP. Must be recognizable at 16x16 and work in monochrome.

### Themes

| Theme | Description |
|-------|-------------|
| Porcelain | Light theme (default) — white surface, deep blue primary |
| Obsidian | Dark theme — dark slate surface, lighter blue primary |
| Aurora | Warm theme — off-white surface, purple + amber accents |
| Follow System | Delegates to OS preference (Porcelain or Obsidian) |

### Implementation

- Colors defined as CSS custom properties (RGB channel values) in `globals.css`.
- Tailwind config references CSS variables via `rgb(var(--token) / <alpha-value>)`.
- Components never hardcode raw color values.
- Theme switching toggles CSS class on root element.

### Deliverables

- Master SVG, monochrome variants, horizontal lockups
- `.ico`, `.icns`, PNG sizes 16–1024, favicon, CLI mark, GitHub social preview

## Consequences

- All user-visible colors are themeable.
- Adding a new theme requires only adding a CSS class with token overrides.
- Logo must pass contrast checks on both light and dark backgrounds.
