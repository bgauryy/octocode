# Review

## Refresh — narrative & density pass (2026-05-10)

The earlier deck was visually mature but had three structural gaps. This pass fixes all three without introducing new content.

### What changed

| Issue | Fix |
|---|---|
| Wix Research section dropped in mid-protocol with no section bracket | Promoted `wix-research-title` after `results-turn-into-next-move` with a new **"Case Study · Wix Engineering"** eyebrow; promoted `wix-research-grounded-answer` as the section closer before returning to general practice |
| Slide 02 (`ai-agents-need-context`) had **8 cards** — Miller's-law violation, noun-phrase title | Hidden; promoted `ai-agents-need-code-context` (4-bullet comparison, claim title) |
| Slide 04 (`octocode-harness`) carried **38 reference items** on one talk slide | Moved to **Appendix** with `[APPENDIX]` stamp + claim-style title; appears after the closing for Q&A reference only |
| Three noun-phrase titles still leaking through | Rewritten as claim sentences (see next section) |
| Content overlap reduced playable density | Hidden 8 redundant slides (typed-findings/wix-helpers, compaction/context-stays-sharp, follow-ups/better-context, etc.) |
| Question → Answer chain broken at the case-study handoff | Updated speaker notes on `results-turn-into-next-move` to verbally bridge into the case study |

### Title sweep — noun-phrase → claim

| Before | After |
|---|---|
| Inside each stage — what actually runs. | **Inside the loop — each stage owns a typed contract.** |
| What changed from search tool to research engine. | **Each layer added a different kind of context.** |
| The hard parts — and what they taught us. | **Four lessons that bent the system into shape.** |
| Tools, skills, and prompts — the full surface area. | **Octocode ships 14 tools, 17 skills, 7 prompts — one harness.** _(now appendix)_ |

### Playable structure

5 acts · 31 visible slides + 1 appendix. Hidden alternates retained for longer-format reuse.

| Act | Slides |
|---|---|
| I · The problem | 01–04 (4) |
| II · Octocode is a research engine | 05–11 (7) |
| III · Case study · Wix Engineering | 12–23 (12) |
| IV · Daily practice | 24–28 (5) |
| V · Lessons + close | 29–31 (3) |
| Appendix (after closing) | 32 (1) |

> 2026-05-10 (later): `wix-research-context-stays-sharp` was pulled from the playable deck. Memory now flows directly into the section closer ("memory carries forward → answer with evidence"), and the case study is one slide tighter.

## Static checks

- **Slide files on disk:** 51 (32 visible + 1 appendix + 18 hidden alternates).
- **Path contract:** every slide links `../css/base.css` and `../css/theme.css` and includes `../js/navbridge.js` before `</body>`.
- **Placeholder tokens:** none (`{{`, `TODO`, `TBD`, `FIXME`, `NEEDS SOURCE` — zero matches).
- **HTML well-formedness:** every slide has exactly one `.slide`, one `<aside class="speaker-notes">`, one `</body>`, one `</html>`.
- **Layout variety:** 8 distinct layout types across the 32 visible slides; no 3 consecutive identical layouts.
- **Manifest format:** all entries `{ path, hidden, name }` with unique slug names; ordered by act, with a separator comment for each.

## Slop tests

### Visual slop — 0 / 8
| # | Signal | Status |
|---|--------|--------|
| 1 | Inter/Roboto headings | IBM Plex Mono throughout |
| 2 | `background-clip: text` gradient on headings | None |
| 3 | Emoji-led bullets/sections | None |
| 4 | Same centered-stack layout everywhere | 8 layout types |
| 5 | Cyan + magenta + purple on dark bg | Cyan + warm + positive (mint) + restrained violet |
| 6 | Animated glowing `box-shadow` on cards | Static `box-shadow`; subtle glow only on focal pips |
| 7 | Three-dot window chrome on every code block | Replaced with `data-label` corner tag |
| 8 | Accent on > 3 elements per slide | One focal accent per slide; lane colours used contextually |

### Content slop — 0 / 8
| # | Signal | Status |
|---|--------|--------|
| 1 | Noun-phrase titles | All visible titles are now claim sentences |
| 2 | Filler language ("leverages", "seamless", "robust") | None |
| 3 | Stat without source | No invented stats; only source-backed claims (90K downloads, 5K users, 11mo, 455K/120K char budgets, top-K 3 / 0.70 relevance) |
| 4 | Slide audience already knew | Each slide delivers a new turn in the argument |
| 5 | Closes on "Thank you" / "Questions?" only | Closing restates code search → intelligence → research engine + 3 takeaway cards |
| 6 | Industry-generic claims | All claims tied to Octocode/Wix research-service mechanics |
| 7 | Decorative diagram (not real structure) | All flows mirror the source content |
| 8 | Decorative image | Bilbo image is the only photo, deliberately framed as the case-study mascot |

## Known limits

- No invented metrics or external claims were added.
- Hidden alternates remain on disk for swap-in (see outline.md "Hidden alternates" section); they are not orphaned.
- The appendix slide (`octocode-harness`) is reachable via overview / hash but skipped during playback unless the speaker navigates to it during Q&A.
