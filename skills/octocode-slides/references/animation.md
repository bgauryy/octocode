# animation.md — In-Slide Step Animation

> Reference for `scripts/animation.js` — the step engine for Octocode Slides.

---

## What it does

`animation.js` lets a single slide reveal its content **one component at a time**, controlled by the same `→` / `←` keys the presenter already uses to navigate between slides.

| Key | Behaviour when steps remain | Behaviour when no steps remain |
|-----|-----------------------------|-------------------------------|
| `→` / `Space` / `↓` | Reveal the next step | Advance to the next slide |
| `←` / `↑` | Hide the last visible step | Retreat to the previous slide |

The slide never advances until all steps have been shown. The slide never retreats until all shown steps have been hidden. Slide authors only opt in per slide; the deck-level controller already knows how to route step-aware keys through the active iframe.

---

## How it works (technical summary)

`animation.js` adds a `keydown` listener in **capture phase** on the slide's `document`. Because it is loaded **before** `navbridge.js`, its handler fires first. When a step is consumed, it calls `e.stopImmediatePropagation()` — `navbridge.js` never sees the event, so the parent (`index.html`) is never told to change slides.

When all steps are exhausted in the current direction, the key passes through normally to navbridge, which forwards it to the parent, and normal slide navigation resumes.

### Full event flow (parent-window focus)

When the parent `index.html` has keyboard focus — initial load, browser chrome refocus, overview close, or other parent-level interaction — raw nav keys would otherwise bypass `animation.js` and jump straight to `go()`. `base.html` avoids that by routing step-aware nav keys through the active iframe first:

```
User presses → / ← (parent window has focus)
  │
  ▼
index.html handleKey()  — no passthrough flag
  └── postMessage {type:'octocode-slides:key', key} to active iframe
          │
          ▼
      navbridge.js message listener  — dispatches synthetic keydown
              │
              ▼
          animation.js (capture, runs first)
              ├── step available in that direction?
              │       └── showNext() / hideLast() + stopImmediatePropagation
              │               └── navbridge keydown listener NEVER fires
              │                       └── no postMessage back → parent stays on slide ✓
              │
              └── no step left in that direction? → falls through
                      │
                      ▼
                  navbridge.js keydown listener
                      └── postMessage {type:'octocode-slides:nav', key} to parent
                              │
                              ▼
                          index.html message handler
                              └── handleKey({passthrough:true}) → go(next/previous) ✓
```

### When the iframe already has focus (user clicked inside the slide)

The same intercept still applies, but the first keydown is real instead of synthetic: `animation.js` intercepts in capture phase, and navbridge forwards only unconsumed keys to the parent.

```
User presses → / ← (iframe has focus)
  │
  ▼
animation.js (capture, runs first)
  ├── step available in that direction? → consume + stopImmediatePropagation → parent stays on slide
  └── no step left in that direction? → fall through
          │
          ▼
      navbridge.js (capture, runs second)
          └── postMessage → parent handleKey({passthrough:true}) → go(next/previous)
```


---

## Required loading order

`animation.js` **must be loaded before `navbridge.js`** so its event listener registers first and intercepts keys before navbridge forwards them.

```html
<!-- In every slide that uses steps — before </body> -->
<script src="../js/animation.js"></script>   <!-- ← FIRST -->
<script src="../js/navbridge.js"></script>   <!-- ← always last -->
```

Slides that do not use steps can omit `animation.js` entirely — navbridge behaviour is unaffected.

---

## Marking steps

Add `data-step="N"` to any element. `N` is a 1-based integer that controls reveal order. Elements without `data-step` are always visible.

```html
<!-- Ordered by data-step value -->
<li data-step="1">First point — revealed on first →</li>
<li data-step="2">Second point — revealed on second →</li>
<li data-step="3">Third point — revealed on third →</li>
```

### DOM-order fallback

If `data-step` has no value (or a non-numeric value), DOM order is used as the fallback. You can mix explicit and implicit ordering — explicit numbers always sort before implicit ones.

```html
<!-- All three reveal in DOM order because no numeric value is given -->
<div data-step>Alpha</div>
<div data-step>Beta</div>
<div data-step>Gamma</div>
```

---

## Default animation

The script injects these defaults via a `<style>` block:

```css
/* Hidden state (before reveal) */
[data-step] {
  opacity: 0;
  transform: translateY(14px);
  transition: opacity 320ms cubic-bezier(.4,0,.2,1),
              transform 320ms cubic-bezier(.4,0,.2,1);
}

/* Visible state (after reveal) */
[data-step].step-visible {
  opacity: 1;
  transform: translateY(0);
}
```

The transition reverses automatically when a step is hidden (on `←`).

---

## Customising the animation

Override the default animation in the slide's local `<style>` block. Define both states — the hidden default AND the visible override:

```html
<style>
  /* Example: slide in from the left */
  [data-step] {
    opacity: 0;
    transform: translateX(-24px);
    transition: opacity 280ms ease, transform 280ms ease;
  }
  [data-step].step-visible {
    opacity: 1;
    transform: translateX(0);
  }
</style>
```

### Staggered delays

Use `transition-delay` on individual elements or CSS nth selectors:

```html
<style>
  [data-step]:nth-of-type(2) { transition-delay: 60ms; }
  [data-step]:nth-of-type(3) { transition-delay: 120ms; }
</style>
```

Or set inline:

```html
<li data-step="1">Fast</li>
<li data-step="2" style="transition-delay:80ms">Delayed</li>
```

---

## Optional step indicator

Add `data-step-indicator` to the slide root element to enable a small dot bar that tracks how many steps have been revealed:

```html
<div class="slide slide--content" data-step-indicator>
  <!-- … slide content … -->
</div>
```

The dot bar appears fixed at the bottom-centre of the slide. Active dots fill white; pending dots are dim. Customise via CSS:

```css
.step-indicator { bottom: 28px; gap: 8px; }       /* reposition / spacing */
.step-dot       { width: 8px; height: 8px; }       /* dot size */
.step-dot.is-done { background: var(--accent); }   /* use theme accent */
```

---

## Full example — content slide with steps

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Three Wins</title>
  <link rel="stylesheet" href="../css/base.css">
  <link rel="stylesheet" href="../css/theme.css">
</head>
<body>
<div class="slide slide--content" data-step-indicator>
  <header class="slide-header">
    <h1>Three things that changed everything</h1>
  </header>

  <main class="slide-content">
    <ul class="content-list">
      <li data-step="1">Deploy time dropped from 45 min to 4 min</li>
      <li data-step="2">Error rate fell 60 % after adding type safety</li>
      <li data-step="3">Team onboarding time cut in half</li>
    </ul>
  </main>
</div>

<!-- Steps load first, navbridge second — order is required -->
<script src="../js/animation.js"></script>
<script src="../js/navbridge.js"></script>
</body>
</html>
```

**Navigation flow for this slide:**
1. Slide loads — all three `<li>` elements are invisible
2. First `→` → "Deploy time…" fades up
3. Second `→` → "Error rate…" fades up
4. Third `→` → "Team onboarding…" fades up
5. Fourth `→` → slide advances to the next slide
6. First `←` (from later slide): retreat to this slide (all steps visible)
7. Next `←` → "Team onboarding…" fades out
8. Next `←` → "Error rate…" fades out
9. Next `←` → "Deploy time…" fades out
10. Next `←` → retreat to the previous slide

---

## Step state on return

When a slide is revisited (e.g. you advanced past it and pressed `←` to come back), its step state is preserved — all previously revealed steps remain visible. This is intentional: returning to a slide you already walked through does not force you to step through it again.

To reset steps, reload the page or navigate away and back across a full reload boundary.

---

## Slides without steps

If a slide has no `[data-step]` elements, `animation.js` is a complete no-op — it adds no visible CSS, registers no effective handler, and does not affect navbridge or slide-change timing. You can safely include `animation.js` in a slide template even if that slide has no steps.

---

## Checklist for slide authors

- [ ] `animation.js` loaded **before** `navbridge.js` in the slide HTML
- [ ] Every step element has `data-step="N"` with a unique integer value
- [ ] Step values are 1-based and sequential (1, 2, 3 …); gaps are allowed (1, 3, 5 …)
- [ ] Content that should always be visible does **not** have `data-step`
- [ ] Slide title / header does **not** have `data-step` (it must be visible on entry)
- [ ] Tested: all steps reveal correctly on `→`, hide correctly on `←`
- [ ] Tested: after last step, next `→` advances to the next slide
- [ ] Tested: before first step, `←` retreats to the previous slide

---

## Integration with the Phase 5 implementation checklist

When building a slide with steps, add this row to the Phase 5 template alignment check:

| Check | Pass condition |
|-------|---------------|
| animation.js loaded before navbridge | `<script src="../js/animation.js">` appears before `<script src="../js/navbridge.js">` in the same slide file |

Do not add `animation.js` to `index.html` or `js/navbridge.js` — it is a per-slide script, not a deck-level script.
