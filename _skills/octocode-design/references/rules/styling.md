# Styling Rules

Best practices for Tailwind CSS, design tokens, and styling patterns.

## Tailwind CSS Conventions

### Use semantic tokens, never raw palette values

```css
/* Correct — semantic, theme-aware */
bg-primary text-primary-foreground
bg-muted text-muted-foreground
bg-destructive text-destructive-foreground
border-border

/* Wrong — hardcoded, breaks dark mode */
bg-blue-500 text-white
bg-gray-100 text-gray-600
bg-red-500
border-gray-200
```

### Spacing: `gap-*` not `space-*`

```tsx
// Correct
<div className="flex flex-col gap-4">

// Wrong — space-y creates issues with conditional children
<div className="space-y-4">
```

### Equal dimensions: `size-*` not `w-* h-*`

```tsx
// Correct
<Avatar className="size-10">

// Wrong — redundant
<Avatar className="w-10 h-10">
```

### Use `cn()` for conditional classes

```tsx
// Correct
<div className={cn("rounded-lg p-4", isActive && "ring-2 ring-primary")}>

// Wrong — manual template literals
<div className={`rounded-lg p-4 ${isActive ? "ring-2 ring-primary" : ""}`}>
```

### No manual `dark:` color overrides

```tsx
// Correct — tokens handle both modes
<div className="bg-background text-foreground">

// Wrong — manual dark mode
<div className="bg-white dark:bg-gray-900 text-black dark:text-white">
```

### `truncate` shorthand

```tsx
// Correct
<p className="truncate">

// Wrong — verbose equivalent
<p className="overflow-hidden text-ellipsis whitespace-nowrap">
```

### No manual z-index on overlays

Overlay components (Dialog, Sheet, Popover, Drawer) handle their own stacking context. Adding `z-index` creates conflicts.

## CSS Custom Properties Architecture

### Token naming convention

```css
:root {
  /* Format: --{category}-{element}-{modifier} */
  --color-background: 0 0% 100%;
  --color-foreground: 0 0% 3.9%;
  --color-primary: 221 83% 53%;
  --color-primary-foreground: 210 40% 98%;
  --color-muted: 210 40% 96%;
  --color-muted-foreground: 215 16% 47%;
  --color-border: 214 32% 91%;
  --color-ring: 221 83% 53%;
  --radius: 0.5rem;
}

.dark {
  --color-background: 0 0% 3.9%;
  --color-foreground: 0 0% 98%;
  /* ... dark overrides */
}
```

### Tailwind v4 theme integration

```css
@import "tailwindcss";

@theme inline {
  --color-background: oklch(var(--color-background));
  --color-foreground: oklch(var(--color-foreground));
  --color-primary: oklch(var(--color-primary));
  --radius-sm: calc(var(--radius) - 4px);
  --radius-md: calc(var(--radius) - 2px);
  --radius-lg: var(--radius);
}
```

## Dark Mode Strategy

### Class-based toggle (recommended)

```tsx
// Toggle dark class on <html>
document.documentElement.classList.toggle("dark");
```

### Image handling in dark mode

```tsx
// Invert diagrams / adjust brightness for photos
<img className="dark:invert" />           // line art, logos
<img className="dark:brightness-90" />    // photos
```

### Semantic token pairs

Every color token must define both light and dark values. Never add a color without its counterpart.

## References

- [shadcn/ui Styling Rules](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/rules/styling.md)
- [Tailwind CSS v4 Docs](https://tailwindcss.com/docs)
- [shadcn/ui Theming](https://ui.shadcn.com/docs/theming)
