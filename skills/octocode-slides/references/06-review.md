# Phase 6 — Review

**Role:** Review agent. Verify the deck is structurally correct, visually consistent, content-accurate, and passes both Slop Tests. Then ask the user to confirm.

**Input:** The full `.octocode/slides/{{slideName}}/` folder

---

## Step 0 · Self-review — run before showing anything to the user

Walk every slide through the three checklists below. Treat them as a thinking tool, not a passing grade — most items are the strong defaults from `SKILL.md → Operating principles → Strong defaults`. Fix every clear failure; when an item is intentionally violated for a brief-driven reason, record the reason inline (in `outline.md → Slide notes` or as an HTML comment) instead of forcing a rewrite.

The "Anti-patterns" table is different — those are auto-fail and should be removed without negotiation.

---

### Content

| Check | Pass condition |
|-------|---------------|
| Title is a claim | Contains a verb + specific assertion — not a topic label ("Performance" fails; "API latency dropped 40% after caching" passes) |
| One idea per slide | Can state the slide's point in a single sentence |
| Text volume | ≤ 6 lines of text on any slide; ≤ 4 bullets; each bullet ≤ 12 words |
| No filler phrases | No "In this slide…", "As you can see…", "Key takeaways:", or similar deck narration |
| No placeholder tokens | No `{{…}}` text remaining in any slide HTML |

---

### UX

| Check | Pass condition |
|-------|---------------|
| 3-second test | Main point is clear without reading every word |
| Visual hierarchy | Largest / boldest element is the most important one; nothing competes equally |
| Layout variety | No 3 consecutive slides with identical layout type |
| Whitespace | ≥ 40% of slide area is empty on every slide |
| No scroll | Every slide fits 1280×720; nothing overflows |

---

### Anti-patterns (auto-fail any of these)

| Pattern | Why it fails |
|---------|-------------|
| Decorative dashes as bullet markers (`—`, `-`, `–` as visual list leaders) | Use the CSS `.bullets` class with `→` from `base.css` instead |
| Inline metadata on content slides (author, date, "Slide X of N" inside the content area) | Belongs in speaker notes or the navigation chrome only |
| Emoji leading bullets or section headers | Breaks visual scale on projectors; use typographic markers |
| `background-clip: text` gradient on headings | AI-gen cliché; remove entirely |
| Accent color on more than 3 elements per slide | Destroys hierarchy |
| All-caps body text | Reduces readability; title/section slides only |
| Hardcoded colors, fonts, or pixel sizes in slide HTML | Must be `var(--token)` only |
| More than 4 bullet points on any slide | Exceeds cognitive chunking limit (Miller's Law) |
| Full paragraphs on any slide | This is a document, not a presentation — use speaker notes |
| Topic-label title (no verb, no specific assertion) | Prefer claim-sentence titles — see slide-rules.md §1.2 |
| Every slide is the same centered-stack layout | Layout monotony kills narrative rhythm |
| Three-dot window chrome (`• • •`) on code blocks | Decorative noise; remove |
| Data slide with no context slide before or after | Data without interpretation creates confusion |
| `{{…}}` placeholder tokens remaining in any HTML | Replace before delivery unless intentionally shown as a labeled placeholder |
| Inter or Roboto as the only heading font | Generic; no design intent; fails Slop Test item 1 |

**Scoring:** Each failing check = one fix to consider. Resolve clear failures before Step 1; document intentional exceptions when the brief requires them.

After fixing all anti-patterns, run both Slop Tests (tables are in `SKILL.md → Slop Test`). Record `Visual Slop: N/8` and `Content Slop: N/8`. Carry both scores into the Step 6 user summary. Visual target is 0/8 and always ≤1/8; Content target is 0/8.

---

## Step 1 · Rendered browser review

Static review is not enough for HTML slides. Before showing the deck to the user, render it.

1. Serve the deck root: `npx serve .octocode/slides/{{slideName}}`
2. Open `index.html` in a browser or browser automation tool
3. Visit every slide through normal navigation and by name hash (e.g. `#title`, `#problem`, `#closing`)
4. Check browser console errors and failed network requests
5. Verify no slide visually overflows or clips important content at 1280×720; when automation is available, compare each iframe document's `scrollWidth/scrollHeight` to its viewport even when CSS uses `overflow: hidden`
6. Verify keyboard controls: next, previous, Home, End, fullscreen, overview, and presenter notes (`P`)
7. Deep-link directly to at least three named hashes in a fresh tab; exactly one `.slide-frame[data-active]` should be active each time
8. Capture screenshots when tooling is available, preferably under `.content/review/screenshots/`

If browser tooling is unavailable, open the deck manually and document that limitation in the final response. If rendering is impossible, tell the user clearly before delivery.

---

## Step 2 · Technical review

Check each item. Fix any failure before moving to Step 3.

**Navigation controller (`index.html`)**
- [ ] `const slides = [...]` uses `{ path, hidden, name }` objects — no plain strings, no numeric names
- [ ] Every slide has a unique `name` slug
- [ ] All `path` values start with `slides/` and the files exist
- [ ] `playable = slides.filter(s => !s.hidden)` filters hidden slides
- [ ] `postMessage` listener handles `octocode-slides:nav` and `octocode-slides:activity`
- [ ] `ResizeObserver` + `scale()` logic present and correct
- [ ] Keyboard navigation: `→` `←` `Space` `Home` `End` `F` `G` `P`
- [ ] Progress bar, counter, and HUD elements present
- [ ] Hash updates use `slide.name` (not numeric index)

- [ ] Direct hash loading activates exactly one iframe and one overview cell

- [ ] `js/presenter.js` is copied, loaded by `index.html`, and `P` opens speaker notes without console errors

- [ ] `index.html` is based on `scripts/base.html` multi-iframe controller, not a stale single-iframe controller

**Pointer + selection contract (CRITICAL — easy to break, breaks silently)**

All `.slide-cell` wrappers stack at `inset:0`. If any inactive cell still accepts pointer events, the topmost stacked cell will swallow every mouse-down and the user cannot select text, click links, or interact with the active iframe. Verify in `index.html` style block:

- [ ] `.slide-cell` declares `pointer-events: none` (default state for inactive cells)
- [ ] `.slide-cell.is-active` declares `pointer-events: auto`
- [ ] `.slide-frame` declares `pointer-events: none`
- [ ] `.slide-frame[data-active]` declares `pointer-events: auto`
- [ ] `go()` toggles BOTH `is-active` (on the cell) AND `data-active` (on the iframe) in the same step — never one without the other
- [ ] No slide HTML or shared CSS sets `user-select: none` on body, `.slide`, or any text-bearing container
- [ ] Browser sanity check: `document.elementFromPoint(640, 360)` (centre of stage) returns the active `<iframe class="slide-frame">`, not a stacked `.slide-cell`. If it returns a cell or another iframe, selection is broken — fix before shipping.

**CSS**
- [ ] `css/base.css` declares all custom properties used across slides
- [ ] `css/theme.css` overrides only variables defined in `base.css`
- [ ] No slide HTML file uses hardcoded `color:`, `font-family:`, or `font-size:` values
- [ ] Google Fonts `@import` is at the top of `theme.css`
- [ ] `@media (prefers-reduced-motion: reduce)` present in `base.css`
- [ ] `@media print` present in `base.css` (1280×720 page size for PDF export)
- [ ] `--text` vs `--bg` contrast ratio is ≥ 4.5:1 WCAG AA (verify using the values in `DESIGN.md` — fail if ratio is not documented or is below 4.5:1)

**Individual slides**
- [ ] Every slide links to `../css/base.css` and `../css/theme.css`
- [ ] Every slide includes `<script src="../js/navbridge.js"></script>` immediately before `</body>`
- [ ] `js/navbridge.js` exists at the deck root

- [ ] `js/presenter.js` exists at the deck root
- [ ] Every CDN library listed in DESIGN.md is loaded in the correct slide files
- [ ] Motion scripts use `type="module"` and load from `cdn.jsdelivr.net/npm/motion@latest/+esm`
- [ ] Code slides initialize highlight.js — either `hljs.highlightAll()` or `querySelectorAll('pre code').forEach(el => hljs.highlightElement(el))`
- [ ] Markdown slides call `marked.parse()` on `[data-md]` elements
- [ ] `<aside class="speaker-notes">` present in every slide
- [ ] Image references use `../assets/filename` (not hardcoded absolute paths)

**Pointer chrome (only if `DESIGN.md → Pointer & click feedback` is present)**
- [ ] `<click-spark>` and `cursor-effects` import live in `index.html` only — never inside any slide HTML
- [ ] `<click-spark>` wrapper has `pointer-events: none` and `z-index` below HUD/progress (HUD = 50, progress = 10, spark ≤ 5)
- [ ] HUD, progress bar, counter, and overview thumbnails remain clickable; spark fires on chrome clicks but does not block them
- [ ] Custom cursor follows pointer with the lag described in DESIGN.md; OS cursor still appears on form fields, links, and during text selection
- [ ] Touch device sanity: `pointer: coarse` short-circuit disables the custom cursor (verify in DevTools mobile emulation)
- [ ] `prefers-reduced-motion: reduce` disables or flattens the spark animation

---

## Step 3 · Design review

**Visual Slop Test** — run the 8-item checklist from `SKILL.md → Slop Test → Visual Slop`. Score ≥ 2 → fix before continuing.

**Visual consistency check**
- [ ] Accent color is used for the same purpose in every slide (e.g., heading only)
- [ ] Font sizes feel consistent slide-to-slide (same heading size hierarchy)
- [ ] Spacing is consistent — padding feels the same on all slides
- [ ] Animations are consistent in timing and feel (not different per slide)
- [ ] Color temperature is consistent (all warm or all cool — no sudden shifts)

**Layout variety check (anti-monotony)**
- [ ] Not every slide is centered-stack
- [ ] At least 2 different layout types used across the deck
- [ ] No 3 consecutive slides with the same layout

---

## Step 4 · Content & flow review

Read `.content/outline.md` rows and compare to the implemented `slides/slug.html` files. Read the full title sequence aloud.

**Outline-to-slide completeness**
- [ ] Every implemented slide has a matching row in `.content/outline.md`
- [ ] No outline row still says `[NEEDS SOURCE]`, `needs asset`, or `revisit` unless the slide intentionally ships a labeled placeholder
- [ ] `Widgets`, `Graphs`, and `Images` notes in the outline match the implemented HTML and loaded libraries

**Content accuracy**
- [ ] Headline text matches what was planned
- [ ] Bullet points match the outline content (no missing or added points)
- [ ] Code slides show the correct language class and snippet
- [ ] Chart/data values match the outline row and cited source
- [ ] Image path, placeholder, alt text, and overlay match the outline notes
- [ ] UX/UI layout, reading order, dominant visual, and animation match the outline notes
- [ ] Speaker notes are present and contain useful context
- [ ] No slide body overflows (verify each layout fits within the 1280×720 stage; split or simplify any slide that would scroll)
- [ ] No placeholder text (`{{…}}` tokens left un-replaced)

**Content Slop Test** — run the 8-item checklist from `SKILL.md → Slop Test → Content Slop`. Score ≥ 1 → fix before continuing.

**Logical flow (slide-rules.md §5)**
- [ ] Ghost outline test: reading only the slide titles tells the complete story (argument → evidence → conclusion)
- [ ] Question-Answer chain: each slide answers the implicit question raised by the previous slide
- [ ] Every `chart`, `stats`, or `code` slide has a context slide before or after it
- [ ] Major topic transitions use a `section` slide — no abrupt jumps between unrelated claims
- [ ] Appendix slides (if any) appear after `closing` and are labeled `[APPENDIX]`
- [ ] No concept is referenced before it is introduced

---

## Step 5 · Fix loop

For each failure found in Steps 1–4:
1. Fix the specific file
2. Re-check only the affected items
3. Confirm: `✓ Fixed: {{description}}`

Re-run the relevant Slop Test after any content, CSS, or theme changes.

---

## Step 6 · Present to user

Once all checks pass:

```
Review complete ✓

Deck: .octocode/slides/{{slideName}}/index.html
Serve: npx serve .octocode/slides/{{slideName}}
Slides: {{N}} · Theme: {{name}} · Visual Slop: {{0 or 1}}/8 · Content Slop: 0/8

Open index.html to see the full presentation.

What would you like to change?
1. Fix a specific slide
2. Change a slide's content or design
3. Add or remove a slide
4. Change the theme
5. It's good — done!
```

If the user says "done" (option 5): confirm the deck location and output the final file tree.

If the user requests changes: make them, re-run the relevant checks from Steps 1–4, then present again.

---

## Step 7 · Final output (on "done")

```
Deck complete ✓

.octocode/slides/{{slideName}}/
├── index.html
├── README.md
├── css/        (base.css + theme.css)
├── js/         (navbridge.js + presenter.js)
├── assets/     (images)
├── slides/     (N slide HTML files)
└── .content/
    └── request.md · outline.md · DESIGN.md

Serve: npx serve .octocode/slides/{{slideName}}
Open:  http://localhost:3000
```
