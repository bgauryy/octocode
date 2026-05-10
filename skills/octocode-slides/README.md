# Octocode Slides

AI agent skill → polished HTML presentation from a plain brief. Local, no cloud, no upload.

---

## Trigger

Say any of these to your agent (Claude, Cursor, Codex…):

```
Create slides about X
Make a presentation from this PDF / notes / codebase
Build a deck explaining Y for [audience]
Generate HTML slides — fast mode       ← skips design approval, just builds
```

---

## Six-phase flow

| Phase | Goal | Agent pauses? | Output |
|-------|------|--------------|--------|
| **1 · Brief** | Extract: audience, goal, depth, sources, constraints | Only if audience/goal missing | `.content/request.md` |
| **2 · Research** | Fill gaps via Octocode / web / local tools. Unknown claims → `[NEEDS SOURCE]` | Rarely | Appended to `request.md` |
| **3 · Outline** | Narrative arc + per-slide rows (title, layout, evidence, notes). Two-pass: top-down then ghost-read | **Yes — confirm structure** | `.content/outline.md` |
| **4 · Design** | 3 style directions → you pick → CSS tokens + library choices | **Yes — pick a direction** | `.content/DESIGN.md` + `css/theme.css` |
| **5 · Implementation** | Build slides from outline rows. 3-second test per slide. Images → placeholder unless you said "generate images" | Only for missing assets | `slides/*.html` + `index.html` |
| **6 · Review** | Visual Slop ≤1/8 · Content Slop 0/8 · navbridge · no `{{…}}` · no overflow | Never | Approved deck + serve command |

**Fast mode** (`"just build it"` / `"your call"` / `"fast mode"`): skips Phase 3/4 pauses, auto-selects theme, still runs Phase 6.

---

## Audience depth

The agent adapts everything to who's in the room:

| Depth | Slides | Style |
|-------|--------|-------|
| Executive | 5–10 | Conclusion-first, stats, no code |
| Management | 10–20 | Charts, evidence, high-level diagrams |
| Technical | 15–30+ | Code, benchmarks, real architecture |
| Async / self-read | any | Self-explanatory titles, denser content, notes as narration |

---

## Slide layout types

| Type | Use when |
|------|----------|
| `title` | Opening — deck name + subtitle |
| `section` | Transition between narrative sections |
| `content` | Default — one claim + supporting bullets |
| `two-col` | Comparison, before/after, text + visual |
| `stats` | 2–4 numbers that carry the point |
| `code` | Real code with syntax highlighting |
| `chart` | Data viz (Chart.js / D3 / Vega-Lite) |
| `image` | Full-bleed visual — screenshot, diagram |
| `timeline` | Sequence, roadmap, history |
| `quote` | Testimonial or pull quote |
| `closing` | CTA — never "Thank you" / "Questions?" |

---

## Quality gates (Phase 6)

Both tests run before every delivery. Fix before showing the user.

### Visual Slop — target 0/8, max 1/8

| # | Auto-fail signal |
|---|-----------------|
| 1 | Inter / Roboto as the only heading font |
| 2 | `background-clip: text` gradient on headings |
| 3 | Emoji leading every bullet or section |
| 4 | Every slide uses the same centered-stack layout |
| 5 | Cyan + magenta + purple on dark background |
| 6 | Animated glowing `box-shadow` on cards |
| 7 | Three-dot window chrome on every code block |
| 8 | Accent color on more than 3 elements per slide |

### Content Slop — 0/8 zero tolerance

| # | Auto-fail signal |
|---|-----------------|
| 1 | Title is a noun phrase, not a claim ("Architecture Overview" not "Caching cut latency 40%") |
| 2 | Bullet contains: "leverages", "seamless", "robust", "innovative", "cutting-edge", "world-class" |
| 3 | Statistic without a source citation |
| 4 | Slide that delivers no new information |
| 5 | Closing slide ends on "Thank you" / "Questions?" with no CTA |
| 6 | Vague claim with no specific number, name, or outcome |
| 7 | Diagram is approximate or invented, not real |
| 8 | Decorative image (stock photo, texture) instead of informational |

---

## Output structure

```
.octocode/slides/<deck-name>/
├── index.html              ← open this in browser
├── css/
│   ├── base.css            ← layout primitives (never edit)
│   └── theme.css           ← deck colors, fonts, tokens
├── js/
│   ├── navbridge.js        ← arrow-key iframe→parent forwarding
│   └── presenter.js        ← P-key presenter popup
├── slides/
│   └── <slug>.html         ← one file per slide
├── assets/                 ← images: ../assets/filename.png
└── .content/
    ├── request.md          ← brief + research
    ├── outline.md          ← narrative arc + every slide row
    └── DESIGN.md           ← visual system + reasoning
```

**Open:** double-click `index.html` in Chrome/Firefox — no server needed.
**Serve:** `npx serve .octocode/slides/<deck-name>`

### Browser controls

| Key | Action |
|-----|--------|
| `→` / `↓` | Next slide |
| `←` / `↑` | Previous slide |
| `P` | Presenter popup (previews + notes + timer + jump) |
| `B` | Blackout |
| `W` | Whiteout |
| Scroll | Navigate |

---

## Editing an existing deck

Agent enters the correct phase — never rebuilds from scratch.

| You say | Agent enters |
|---------|-------------|
| "Review" / "what's wrong" / "audit" | Phase 6 |
| "Fix this slide" / "update content" | Phase 5 → re-run Phase 6 |
| "Add / remove a slide" | Phase 3 → 5 |
| "Change theme / colors / fonts" | Phase 4 |
| "Restructure" / "reorder" | Phase 3 |

---

## Image generation (opt-in)

**Never silent.** Say "generate images" explicitly.

| Path | Auth | Best for |
|------|------|----------|
| **A — Python SDK** (default) | `GEMINI_API_KEY` | Automated builds, scripting |
| **B — `belt` CLI** | inference.sh account | One-liners, Google Search grounding |
| **C — Gemini CLI + MCP** | GCP project + ADC (`gcloud auth application-default login`) | Conversational sessions in Gemini CLI |

### Path A setup

#### 1. Get API key → [aistudio.google.com/apikey](https://aistudio.google.com/apikey)

#### 2. Set `GEMINI_API_KEY`

**macOS / Linux**
```bash
# Temporary
export GEMINI_API_KEY="your-key-here"

# Permanent — zsh (macOS default)
echo 'export GEMINI_API_KEY="your-key-here"' >> ~/.zshrc && source ~/.zshrc

# Permanent — bash
echo 'export GEMINI_API_KEY="your-key-here"' >> ~/.bashrc && source ~/.bashrc

# Verify
echo $GEMINI_API_KEY
```

**Windows**
```powershell
# Temporary (PowerShell session)
$env:GEMINI_API_KEY = "your-key-here"

# Permanent (PowerShell — restart terminal after)
[System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", "your-key-here", "User")

# Permanent (GUI): Win+S → "Edit the system environment variables"
# → Environment Variables → User variables → New
# Name: GEMINI_API_KEY   Value: your-key-here → OK → restart terminal

# Verify
echo $env:GEMINI_API_KEY
```

#### 3. Install `uv` (auto-installs Python deps on first run)

```bash
# macOS / Linux
brew install uv
# or
curl -LsSf https://astral.sh/uv/install.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"

# Verify
uv --version
```

#### 4. Use it

Ask agent: `Create slides about X — generate images`

Or call directly:
```bash
cd .octocode/slides/my-deck

uv run /path/to/skills/octocode-slides/scripts/generate_image.py \
  --prompt "Cinematic wide shot, modern server room, blue hour, no text" \
  --filename "assets/hero.png" \
  --resolution 2K \
  --aspect-ratio 16:9
```

**Script flags:**

| Flag | Required | Values |
|------|----------|--------|
| `--prompt` / `-p` | Yes | Generation prompt |
| `--filename` / `-f` | Yes | Output path in `assets/` |
| `--resolution` / `-r` | No | `512px` · `1K` (default) · `2K` · `4K` |
| `--aspect-ratio` / `-a` | No | `16:9` · `1:1` · `4:5` · `2:3` · … |
| `--input-image` / `-i` | No | Reference image — repeat up to 14× |
| `--api-key` / `-k` | No | Override `GEMINI_API_KEY` for this call |

Resolution guidance: `512px` → draft/iterate · `1K` → inline/decorative · `2K` → two-col half · `4K` → full-bleed only.

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `Error: No API key provided` | `echo $GEMINI_API_KEY` — empty? Set it (Step 2 above) |
| `uv: command not found` | Install `uv` (Step 3 above) |
| `ModuleNotFoundError: google` | Use `uv run`, not `python` — uv installs deps automatically |
| Slides don't open | Double-click `index.html` in Chrome or Firefox |
| Arrow keys stop working after click | Click inside the slide (iframe needs focus) |
| Images not generating | Say "generate images" — agent never generates silently |
| Slide content overflows | Agent splits into a new slide — max 1280×720, no scrolling |
| `[NEEDS SOURCE]` in slide | Agent hit an unverifiable claim — provide the source or confirm the data |

---

## References

- [Gemini API key](https://aistudio.google.com/apikey) · [uv](https://astral.sh/uv) · [Nano Banana 2 model docs](https://ai.google.dev/gemini-api/docs/image-generation)
- [Image generation reference](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-slides/references/image-generation.md)
- [SKILL.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-slides/SKILL.md) · [slide-rules.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/octocode-slides/references/slide-rules.md)
