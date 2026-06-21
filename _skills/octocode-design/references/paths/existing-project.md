# Existing Project Path - Scan, Generate, Improve

Use this path when the project already has meaningful UI code, tokens, components, or pages.

## Goal

Create a project-specific `DESIGN.md` from real implementation evidence, then propose prioritized improvements.

## Flow

`DISCOVER -> ANALYZE -> DESIGN DECISIONS -> GENERATE -> VALIDATE -> IMPROVE`

## 1) Discover With Octocode MCP

Run these with local tools first:

- `localViewStructure`: map root, `src`, `app`, `components`, `styles`
- `localFindFiles`: locate `package.json`, CSS files, Tailwind config, theme files
- `localSearchCode`: find `--color`, `font-family`, spacing patterns, component variants

Also run `octocode-engineer` mindset:
- system first
- map ownership and blast radius
- avoid one-file assumptions

Capture:
- framework + rendering model
- styling system
- token locations
- component architecture
- dark mode mechanism

## 2) Analyze What Exists

Build an inventory table before writing:

- Color tokens and raw color usage
- Type scale and font stack
- Spacing scale and layout rhythm
- Existing components and states
- Accessibility patterns (focus, keyboard, ARIA)
- Motion usage and performance impact

Mark each item:
- `keep` (already strong)
- `fix` (needs improvement)
- `missing` (must be defined)

## 3) Design Decisions (Think Like a Designer)

For every major section:
- describe intent and emotional effect
- define exact values and constraints
- explain tradeoff

Do not erase current product identity unless user asks for redesign.

## 4) Generate `DESIGN.md`

Use the 13-section skeleton from `SKILL.md`, but fill content from project evidence.

For each section include:
- current state snapshot
- target rule
- migration hint (if current implementation differs)

## 5) Validate Against Code

Use `localSearchCode` anti-pattern checks:
- `bg-blue|bg-red|bg-green|text-blue|text-red`
- `space-y-|space-x-`
- `dark:`
- `style=`

Also verify:
- token completeness (light/dark)
- accessibility thresholds
- motion/performance consistency

## 6) Improvement Suggestions (Required)

After `DESIGN.md`, provide:

1. top 5 design-system improvements
2. why each matters (UX/dev speed/consistency/accessibility)
3. suggested implementation order (high -> low impact)

Each suggestion should be concrete and codebase-aware, not generic advice.
