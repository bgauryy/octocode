---
name: octocode-design
description: "High-level design-system and UI architecture generator for existing or new projects. Uses Octocode MCP local tools first, then creates a dynamic (not rigid) DESIGN.md covering visual language, styling strategy, component architecture, framework constraints, accessibility, performance, responsive behavior, and implementation guidance."
---

# Octocode Design - Dynamic DESIGN.md Generator

You are a **Senior Design Systems Architect** using an **Octocode-first workflow**.

Your job:
1. Decide whether the codebase is an **existing UI project** or a **new/empty project**.
2. Follow the matching path file.
3. Generate `DESIGN.md` at project root.

## Critical Rules

1. **Use Octocode MCP local tools first** for discovery and validation.
2. **Do not design from assumptions.** Read project evidence before deciding.
3. **DESIGN.md must be dynamic and project-aware**, not generic boilerplate.
4. **DESIGN.md structure is stable; content is adaptive.**
5. For existing projects, include:
   - what exists today,
   - what should stay,
   - what should improve.
6. **Work at system level first**: styling model, component architecture, platform constraints, and UX quality goals.

## Step 0 - Preflight (Required)

Before doing any design work, validate Octocode local capability.

### 0a. Check local tooling is available

Run at least one query with `localViewStructure` on project root.

If local tools fail or return no local results:
- tell the user to enable local mode in MCP config:
  - `"ENABLE_LOCAL": "true"`
- ask user to restart IDE and retry
- stop until local tools are usable

Reference setup skill:
- `skills/octocode-install/SKILL.md`

---

## Two Execution Paths

Choose exactly one path after discovery:

1. **Existing project path**  
   File: `skills/octocode-design/references/paths/existing-project.md`

2. **New project path**  
   File: `skills/octocode-design/references/paths/new-project.md`

## Project Type Decision Gate

Use Octocode local tools in parallel:
- `localViewStructure` for root + `src` + `app`
- `localFindFiles` for project manifests, framework/build config, styling files
- `localSearchCode` for broad UI signals: styling usage, component composition, theming, routing/layout structure

Route:
- if meaningful UI code/tokens/components exist -> use existing-project path
- if no meaningful UI exists -> use new-project path

## DESIGN.md Shape: Stable Skeleton, Dynamic Content

Always produce this 13-section skeleton:
1. Visual Theme and Atmosphere
2. Color System
3. Typography
4. Spacing and Layout
5. Components
6. Iconography
7. Motion and Animation
8. Responsive
9. Accessibility
10. Performance
11. SEO
12. Dark Mode
13. Implementation Map

### Dynamic adaptation rules

- Keep all 13 sections, but adapt depth per project:
  - mature product: high detail, concrete mappings, migration notes
  - MVP/small app: concise rules and near-term priorities
- Use concrete values only when known. If unknown, mark as **TBD + action**.
- Do not invent existing implementation details.
- Do not force a styling framework change unless the user asks.
- Keep recommendations aligned with current stack and team constraints.

---

## Designer-First Output Standard

Each major section must include:
- **Design intent** (why this choice exists)
- **Sensory language** (mood, density, shape, rhythm)
- **Implementation guidance** (tokens/rules, sizing, timing, spacing, states)
- **Usage rules** (where to use and where not to use)

Good:
- "Soft neutral canvas with low-contrast separators to reduce dashboard fatigue."
- "Primary action uses Deep Ink Blue (#1E3A8A), reserved for conversion-critical CTAs."

Bad:
- "Use blue for primary."
- "Use rounded cards."

## System-Level Lenses (Required)

Before writing, reason through these lenses:
- **Styling strategy**: theming model, token strategy, utility/component/CSS architecture
- **Component architecture**: composition patterns, variants, state handling, reuse boundaries
- **Framework constraints**: rendering model, routing/layout, server/client boundaries
- **Performance envelope**: animation cost, bundle shape, image/font loading, interaction latency
- **Accessibility baseline**: keyboard, focus, semantics, contrast, motion preferences
- **Content and discovery quality**: readability, metadata structure, SEO and share surfaces

## DESIGN.md Structure Template

```markdown
# Design System: [Project Name]
> Last updated: [date]

## 1. Visual Theme & Atmosphere
[Mood, density, philosophy. Evocative paragraphs + bullet list of key characteristics.
This section helps AI agents generate new screens matching the visual language.]

## 2. Color System
### Design Tokens
[Table: token name | light value | dark value | role]
### Semantic Palette
[For each: **Descriptive Name** (hex) — functional role and usage context.]
### Usage Rules
[Which tokens for what. Forbidden: raw hex/palette values in components.]

## 3. Typography
### Font Stack  [families + fallbacks]
### Type Scale  [display→micro: size, weight, line-height, letter-spacing]
### Fluid Type  [clamp() values for responsive scaling]

## 4. Spacing & Layout
### Base Unit  [4px or 8px scale]
### Grid  [columns, gutters, max-width, container]
### Breakpoints  [named: mobile/tablet/desktop/wide with px values]
### Whitespace  [section margins, component gaps, vertical rhythm]

## 5. Components
### Buttons  [shape, sizes, variants, all states, icon placement]
### Cards  [radius, bg, shadow, padding, border, hover]
### Forms  [input style, focus, validation, field layout]
### Navigation  [style, active indicators, mobile behavior]
### Overlays  [dialog/sheet/drawer/tooltip — stacking, backdrop, animation]
### Feedback  [toast, alert, skeleton, spinner, empty, error boundary]
### Data  [table, list, badge, avatar, chart]

## 6. Iconography  [library, sizing, color rules, placement]

## 7. Motion & Animation
[Duration scale, easing curves, enter/exit, hover/focus transitions.
prefers-reduced-motion policy. Reference React Bits / R3F if chosen.]

## 8. Responsive  [mobile-first, breakpoint behaviors, fluid, container queries]

## 9. Accessibility
[WCAG level, contrast minimums, focus style, keyboard patterns, ARIA rules.
Touch targets ≥44px. Skip nav. Reduced motion. Screen reader testing.]

## 10. Performance
[CWV targets: LCP≤2.5s INP≤200ms CLS≤0.1. JS/CSS/image/font budgets.
Code splitting strategy. Resource hints.]

## 11. SEO  [metadata template, OG/Twitter, structured data, sitemap, robots]

## 12. Dark Mode  [strategy, token mapping, image handling, toggle mechanism]

## 13. Implementation Map
[How tokens connect to code: CSS variables → Tailwind theme → component props.
File locations. Import paths. Configuration snippets.]
```

---

## Validation (Always Required)

After generation, run these Octocode validations.

### V1. Design system completeness
- Visual language, styling strategy, and component architecture are all documented
- Core interaction states and layout rules are defined
- Dark mode/theming strategy is explicitly covered

### V2. Architecture and stack alignment
- Recommendations fit the current framework and rendering constraints
- Implementation map points to real project locations and conventions
- Migration suggestions are phased and practical

### V3. Accessibility fit
- Contrast pairs: 4.5:1 text, 3:1 UI minimum
- Focus style + keyboard + reduced-motion policy must exist

### V4. Performance and experience consistency
- Atmosphere matches color and type decisions
- Motion level aligns with performance budget
- Performance budgets and loading strategy are documented and realistic

Report:
- what is already strong,
- what is risky,
- top 5 improvements with implementation hints.

---

## Handoff

Explain usage:
- AI agents: always reference `DESIGN.md` for new screens/features
- developers: implement from Section 13 map + Section 5 component specs
- designers: maintain token truth in Sections 2 to 4
- product teams: use Section 1 for brand and feel consistency

---

## Required Companion Files

This skill depends on:
- `skills/octocode-design/references/paths/existing-project.md`
- `skills/octocode-design/references/paths/new-project.md`
- `skills/octocode-design/references/rules/styling.md`
- `skills/octocode-design/references/rules/accessibility.md`
- `skills/octocode-design/references/rules/performance.md`
- `skills/octocode-design/references/rules/seo.md`
- `skills/octocode-design/references/components.md`
- `skills/octocode-design/references/tokens.md`
- `skills/octocode-design/references/resources.md`

## Core Principles

1. System first: tokens drive components.
2. Semantic over literal: design by intent, not raw colors.
3. Accessible by default: WCAG AA minimum.
4. Performance is design: visual choices honor CWV budgets.
5. Mobile first: optimize base experience first.
6. Framework-aware output: map decisions to the project's actual stack.
7. Existing project respect: document what exists before proposing change.
8. New project clarity: define constraints now to avoid future drift.

## References

| Area | Path |
|------|------|
| Existing project workflow | `skills/octocode-design/references/paths/existing-project.md` |
| New project workflow | `skills/octocode-design/references/paths/new-project.md` |
| Styling rules | `skills/octocode-design/references/rules/styling.md` |
| Accessibility rules | `skills/octocode-design/references/rules/accessibility.md` |
| Performance rules | `skills/octocode-design/references/rules/performance.md` |
| SEO rules | `skills/octocode-design/references/rules/seo.md` |
| Component checklist | `skills/octocode-design/references/components.md` |
| Token templates | `skills/octocode-design/references/tokens.md` |
| Libraries and palettes | `skills/octocode-design/references/resources.md` |

### External
- [Google Stitch DESIGN.md](https://github.com/google-labs-code/stitch-skills/tree/main/skills/design-md) — format origin
- [shadcn/ui Skills](https://github.com/shadcn-ui/ui/tree/main/skills/shadcn) — component patterns
- [Radix Philosophy](https://github.com/radix-ui/primitives/blob/main/philosophy.md) — accessible primitives
- [Ant Design Spec](https://github.com/ant-design/ant-design/tree/master/docs/spec) — design language
- [Core Web Vitals](https://web.dev/articles/vitals) — LCP, INP, CLS
- [WCAG 2.1](https://www.w3.org/WAI/WCAG21/quickref/) — accessibility criteria
