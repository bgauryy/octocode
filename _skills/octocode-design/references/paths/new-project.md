# New Project Path - Define and Create

Use this path when there is no meaningful existing UI implementation.

## Goal

Create a clear, build-ready `DESIGN.md` that gives teams and AI agents a shared design system from day one.

## Flow

`DISCOVER -> DECIDE -> GENERATE -> VALIDATE -> HANDOFF`

## 1) Minimal Discovery With Octocode MCP

Even for new projects, run discovery first:
- `localViewStructure` for project layout
- `localFindFiles` for framework or starter templates
- `localSearchCode` to ensure no hidden token/style baseline exists

If early UI exists, switch to existing-project path.

## 2) Decide Direction (Designer-First)

Use structured user input (`AskQuestion` when available):

- Visual mood
- Product type
- Brand personality
- Motion intensity
- Accessibility strictness

Then choose:
- color strategy
- type system
- spacing/grid
- component style language
- dark mode strategy

Every choice needs:
- rationale
- measurable values
- usage boundaries

## 3) Generate `DESIGN.md`

Use full 13 sections, but keep scope practical for phase-1 delivery.

When unknown:
- mark as `TBD`
- add clear action owner + decision trigger

Avoid fake precision for unknown data.

## 4) Validate Internal Consistency

Check:
- palette and atmosphere alignment
- type + spacing harmony
- motion vs performance budget
- dark mode completeness
- accessibility baseline readiness

## 5) Handoff

Provide:
- how developers should apply tokens and components
- how AI agents should reference `DESIGN.md`
- what must be decided before first production release

This path should produce a clear starting system, not a rigid final doctrine.
