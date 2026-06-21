# Accessibility Rules

WCAG 2.1 AA compliance as the minimum bar for all design decisions.

## Core Principles

1. **Native HTML first** — `<button>`, `<a>`, `<input>`, `<nav>`, `<main>` before ARIA
2. **No ARIA is better than bad ARIA** — Only add ARIA when native semantics fall short
3. **Keyboard-first** — Every interactive element must be reachable and operable via keyboard
4. **Test with real assistive tech** — Screen readers (VoiceOver, NVDA), not just automated tools

## Color & Contrast

| Element | Minimum ratio | WCAG criterion |
|---------|--------------|----------------|
| Normal text (<18px) | 4.5:1 | 1.4.3 AA |
| Large text (>=18px bold or >=24px) | 3:1 | 1.4.3 AA |
| UI components & graphics | 3:1 | 1.4.11 AA |
| Focus indicators | 3:1 against adjacent | 1.4.11 AA |

- Never convey information through color alone (add icons, patterns, or text)
- Test contrast with both light and dark themes

## Focus Management

### Visible focus indicators (required)

```css
/* Minimum: 2px outline with contrast */
:focus-visible {
  outline: 2px solid var(--color-ring);
  outline-offset: 2px;
}

/* Remove default only when custom is provided */
:focus:not(:focus-visible) {
  outline: none;
}
```

### Focus trapping in overlays

Dialogs, sheets, and drawers must trap focus:
- Focus moves to first focusable element on open
- Tab cycles within the overlay
- Escape closes and returns focus to trigger
- Radix primitives handle this automatically

### Skip navigation

```tsx
<a href="#main-content" className="sr-only focus:not-sr-only focus:absolute focus:z-50 focus:p-4">
  Skip to main content
</a>
```

## Keyboard Patterns

| Component | Keys |
|-----------|------|
| Button | Enter, Space -> activate |
| Link | Enter -> navigate |
| Menu | Arrow keys -> navigate, Enter -> select, Escape -> close |
| Tabs | Arrow keys -> switch tab, focus follows selection |
| Dialog | Escape -> close, Tab -> cycle focus |
| Combobox | Arrow keys -> options, Enter -> select, Escape -> close |
| Toggle | Space -> toggle |

## ARIA Essentials

### Landmarks

```html
<header role="banner">       <!-- or just <header> at top level -->
<nav role="navigation">       <!-- or just <nav> -->
<main role="main">            <!-- or just <main> -->
<footer role="contentinfo">   <!-- or just <footer> -->
<aside role="complementary">  <!-- or just <aside> -->
```

### Required ARIA for common patterns

```tsx
// Dialogs — always need a title
<Dialog>
  <DialogTitle>Edit Profile</DialogTitle>           {/* visible */}
  <DialogTitle className="sr-only">Settings</DialogTitle> {/* hidden OK */}
</Dialog>

// Loading states
<div aria-busy="true" aria-live="polite">Loading...</div>

// Form validation
<Field data-invalid>
  <Input aria-invalid="true" aria-describedby="email-error" />
  <span id="email-error" role="alert">Invalid email</span>
</Field>

// Icon-only buttons
<Button aria-label="Close menu">
  <XIcon data-icon />
</Button>
```

### Live regions

```tsx
// Polite — announced at next pause (status updates, search results count)
<div aria-live="polite" aria-atomic="true">{count} results found</div>

// Assertive — announced immediately (errors, critical alerts)
<div aria-live="assertive" role="alert">{error}</div>
```

## Reduced Motion

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}
```

Always check `prefers-reduced-motion` before complex animations.

## Responsive Accessibility

- Touch targets minimum 44x44px (WCAG 2.5.8)
- Zoom to 200% without horizontal scroll (WCAG 1.4.10)
- Text resizable to 200% without loss of content (WCAG 1.4.4)

## Testing Checklist

- [ ] All interactive elements keyboard-reachable
- [ ] Visible focus indicators on every focusable element
- [ ] Color contrast passes for all text and UI elements
- [ ] All images have meaningful `alt` text (or `alt=""` for decorative)
- [ ] Form inputs have associated labels
- [ ] Error messages are programmatically associated and announced
- [ ] Page has proper heading hierarchy (h1 -> h2 -> h3)
- [ ] Landmark regions present and correct
- [ ] Dialogs trap focus and have accessible names
- [ ] Reduced motion preference respected

## References

- [MDN ARIA Guide](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA)
- [WAI-ARIA Authoring Practices Guide](https://www.w3.org/WAI/ARIA/apg/)
- [WCAG 2.1 Quick Reference](https://www.w3.org/WAI/WCAG21/quickref/)
- [Radix Philosophy — Accessibility](https://github.com/radix-ui/primitives/blob/main/philosophy.md)
