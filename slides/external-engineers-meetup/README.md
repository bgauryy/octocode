# From Code Search to AI-Powered Research Engine

40-visible-slide technical keynote on how Octocode evolved from GitHub code search into an AI-powered research engine, now with a Wix Research / Bilbo section inserted into the research-protocol arc.

## Serve
```
npx serve slides/external-engineers-meetup
```
Then open `http://localhost:3000`.

## Keys
| Key | Action |
|-----|--------|
| `→` / `Space` | Next slide |
| `←` | Previous slide |
| `Home` / `End` | First / last slide |
| `G` | Toggle overview grid |
| `F` | Fullscreen |
| `P` | Presenter notes |

## Structure
| File | Purpose |
|------|---------|
| `index.html` | Navigation controller (loads slides as iframe grid, hashes, presenter notes) |
| `slides/*.html` | One file per slide; playback order is controlled by `index.html` |
| `css/base.css` | Layout, components, animations, syntax-token classes |
| `css/theme.css` | Colors and font tokens (override here to retheme) |
| `js/navbridge.js` | Keeps keyboard navigation working inside slide iframes |
| `js/presenter.js` | Presenter notes popup opened with `P` |
| `.content/` | Brief, research, outline, design notes, per-slide specs, review |

## Act structure
- **Slides 02–11 — Act I · Context problem and code-intelligence loop** (eyebrow on slide 02)
- **Slides 12–23 — Act II · Octocode as the research engine** (eyebrow on slide 12)
- **Slides 24–29 — Act III · Daily usage and operating lessons** (eyebrow on slide 24)
- **Slide 30** — Closing: `code search → code intelligence → research engine`

## Wix Research / Bilbo inserted section
- 14 visible slides from `.content/INTRODUCING_WIX_RESEARCH.md` are inserted after `#protocol-response-hints` and before `#planner-researcher-contract`.
