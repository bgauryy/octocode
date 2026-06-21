# Design Token Reference

How to structure and implement design tokens in DESIGN.md.

## Token Categories

| Category | Examples | CSS variable prefix |
|----------|---------|-------------------|
| **Color** | Background, foreground, primary, accent | `--color-*` |
| **Typography** | Font family, size scale, weights | `--font-*`, `--text-*` |
| **Spacing** | Padding, margin, gap scale | `--spacing-*` |
| **Radius** | Border radius scale | `--radius-*` |
| **Shadow** | Elevation levels | `--shadow-*` |
| **Animation** | Duration, easing | `--duration-*`, `--ease-*` |
| **Z-index** | Stacking layers | `--z-*` |

## Color Token Template

### Semantic color tokens (required)

```css
:root {
  /* Surfaces */
  --color-background: #ffffff;
  --color-foreground: #0a0a0a;
  --color-card: #ffffff;
  --color-card-foreground: #0a0a0a;
  --color-popover: #ffffff;
  --color-popover-foreground: #0a0a0a;

  /* Brand */
  --color-primary: #171717;
  --color-primary-foreground: #fafafa;
  --color-secondary: #f5f5f5;
  --color-secondary-foreground: #171717;
  --color-accent: #f5f5f5;
  --color-accent-foreground: #171717;

  /* Semantic */
  --color-destructive: #ef4444;
  --color-destructive-foreground: #fafafa;
  --color-success: #10b981;
  --color-success-foreground: #fafafa;
  --color-warning: #f59e0b;
  --color-warning-foreground: #0a0a0a;
  --color-info: #3b82f6;
  --color-info-foreground: #fafafa;

  /* UI elements */
  --color-muted: #f5f5f5;
  --color-muted-foreground: #737373;
  --color-border: #e5e5e5;
  --color-input: #e5e5e5;
  --color-ring: #171717;
}

.dark {
  --color-background: #0a0a0a;
  --color-foreground: #fafafa;
  /* ... every token must have a dark counterpart */
}
```

## DESIGN.md Color Documentation Format

For each color in the `DESIGN.md`, use this format:

```markdown
- **Descriptive Name** (#hexcode) — Functional role.
  Usage: Where and when to use it.
```

Example:
```markdown
### Semantic Palette

- **Warm Barely-There Cream** (#FCFAFA) — Primary background.
  Creates warmth that feels more inviting than pure white.

- **Deep Muted Teal-Navy** (#294056) — Primary CTA and accent.
  Used for buttons, active nav links, and selected filter states.
```

## Typography Token Template

```css
:root {
  /* Font families */
  --font-sans: "Inter", ui-sans-serif, system-ui, sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, monospace;

  /* Font sizes (fluid) */
  --text-xs: clamp(0.7rem, 0.65rem + 0.25vw, 0.75rem);
  --text-sm: clamp(0.8rem, 0.75rem + 0.25vw, 0.875rem);
  --text-base: clamp(0.9rem, 0.85rem + 0.25vw, 1rem);
  --text-lg: clamp(1rem, 0.9rem + 0.5vw, 1.125rem);
  --text-xl: clamp(1.1rem, 1rem + 0.5vw, 1.25rem);
  --text-2xl: clamp(1.3rem, 1.1rem + 1vw, 1.5rem);
  --text-3xl: clamp(1.6rem, 1.3rem + 1.5vw, 1.875rem);
  --text-4xl: clamp(2rem, 1.5rem + 2.5vw, 2.25rem);

  /* Line heights */
  --leading-tight: 1.25;
  --leading-normal: 1.5;
  --leading-relaxed: 1.7;

  /* Letter spacing */
  --tracking-tight: -0.02em;
  --tracking-normal: 0;
  --tracking-wide: 0.02em;
}
```

## Spacing Token Template

```css
:root {
  /* 4px base unit scale */
  --spacing-0: 0;
  --spacing-1: 0.25rem;   /* 4px */
  --spacing-2: 0.5rem;    /* 8px */
  --spacing-3: 0.75rem;   /* 12px */
  --spacing-4: 1rem;      /* 16px */
  --spacing-5: 1.25rem;   /* 20px */
  --spacing-6: 1.5rem;    /* 24px */
  --spacing-8: 2rem;      /* 32px */
  --spacing-10: 2.5rem;   /* 40px */
  --spacing-12: 3rem;     /* 48px */
  --spacing-16: 4rem;     /* 64px */
  --spacing-20: 5rem;     /* 80px */
  --spacing-24: 6rem;     /* 96px */
}
```

## Radius Token Template

```css
:root {
  --radius: 0.5rem;       /* Base radius */
  --radius-sm: calc(var(--radius) - 4px);
  --radius-md: calc(var(--radius) - 2px);
  --radius-lg: var(--radius);
  --radius-xl: calc(var(--radius) + 4px);
  --radius-full: 9999px;  /* Pill shape */
}
```

## Shadow Token Template

```css
:root {
  --shadow-none: none;
  --shadow-xs: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.1), 0 1px 2px rgba(0, 0, 0, 0.06);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.1), 0 2px 4px rgba(0, 0, 0, 0.06);
  --shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.1), 0 4px 6px rgba(0, 0, 0, 0.05);
  --shadow-xl: 0 20px 25px rgba(0, 0, 0, 0.1), 0 10px 10px rgba(0, 0, 0, 0.04);
}
```

## Animation Token Template

```css
:root {
  /* Durations */
  --duration-fast: 150ms;
  --duration-normal: 250ms;
  --duration-slow: 350ms;
  --duration-slower: 500ms;

  /* Easing */
  --ease-in: cubic-bezier(0.4, 0, 1, 1);
  --ease-out: cubic-bezier(0, 0, 0.2, 1);
  --ease-in-out: cubic-bezier(0.4, 0, 0.2, 1);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
}
```

## Tailwind v4 Integration

```css
@import "tailwindcss";

@theme inline {
  --color-background: var(--color-background);
  --color-foreground: var(--color-foreground);
  --color-primary: var(--color-primary);
  --color-primary-foreground: var(--color-primary-foreground);
  --font-sans: var(--font-sans);
  --radius-lg: var(--radius);
  --radius-md: var(--radius-md);
  --radius-sm: var(--radius-sm);
}
```

## References

- [shadcn/ui Theming](https://ui.shadcn.com/docs/theming)
- [Tailwind CSS v4 Theme](https://tailwindcss.com/docs/theme)
- [Google Stitch — DESIGN.md Example](https://github.com/google-labs-code/stitch-skills/blob/main/skills/design-md/examples/DESIGN.md)
