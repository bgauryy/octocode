# Image Generation — Nano Banana 2 (Gemini 3.1 Flash Image)

> **Status:** Opt-in. The default image rule in `references/05-implementation.md` is still "do not silently generate". This doc tells you **how** to generate when the user explicitly asks for it, and how to keep the output consistent with the deck's design system.

---

## TL;DR — which path

| Path | Use when | Auth | Effort |
|------|----------|------|--------|
| **A — Direct API via `google-genai` SDK** (default) | Standard case. Stable, scriptable, no third party. | `GEMINI_API_KEY` (AI Studio) | Add `scripts/generate_image.py`, set `GEMINI_API_KEY`. |
| **B — Third-party CLI (`belt` from inference.sh)** | User already on inference.sh, wants one-liner shell calls, or needs Google Search grounding out of the box. | `belt login` (inference.sh account) | `npm i -g @inferencesh/cli`, `belt login`. |
| **C — Gemini CLI + `mcp-nanobanana-go` MCP server** | User is already using Gemini CLI day-to-day and has a GCP project. Image generation via native MCP tool-calling. | Google Cloud ADC (`gcloud auth application-default login`) + `GOOGLE_CLOUD_PROJECT` | Install the Go binary MCP server + configure `~/.gemini/settings.json`. See Path C below. |

> **Note on the official `gemini` CLI**: The CLI itself does **not** have a built-in `generate image` command. Images are only possible via Path C (an MCP server that plugs into Gemini CLI). Do not propose "just run `gemini generate image …`" — that does not exist.

**Model code:** `gemini-3.1-flash-image-preview` (Nano Banana 2). For maximum factual accuracy / studio quality, swap to `gemini-3-pro-image-preview` (Nano Banana Pro). Legacy = `gemini-2.5-flash-image` (Nano Banana 1, original).

---

## When to invoke during the six-phase flow

Image generation is a **Phase 5 (Implementation)** decision, never a Phase 1–4 ask. Trigger conditions, all required:

1. The user has **explicitly opted in** ("generate images", "make the hero image", "create the assets"). Implicit opt-in (e.g. `slide notes` say `IMAGE`) is not enough.
2. The slide's outline row gives a concrete `data-expected` description (subject, style, mood, composition).
3. The deck's `DESIGN.md` has a locked aesthetic — colors, lighting, style — so generated images don't fight the theme.
4. Required credentials are available: `GEMINI_API_KEY` (Path A/B) or GCP ADC + `GOOGLE_CLOUD_PROJECT` (Path C).

If any condition is missing → keep the `image-ph` / `image-ph-bleed` placeholder. Never half-generate.

---

## Path A — Direct API (recommended)

### Setup

Drop a one-file helper into the **skill** (not the deck) so every generated deck shares the same script:

```
skills/octocode-slides/scripts/generate_image.py   ← new helper
```

The helper is a thin wrapper around `google-genai`:

```python
from google import genai
from google.genai import types
client = genai.Client(api_key=os.environ["GEMINI_API_KEY"])
response = client.models.generate_content(
    model="gemini-3.1-flash-image-preview",
    contents=[prompt, *reference_images],
    config=types.GenerateContentConfig(
        response_modalities=["TEXT", "IMAGE"],
        image_config=types.ImageConfig(image_size=resolution, aspect_ratio=ratio),
    ),
)
```

Full reference implementation: [`intellectronica/agent-skills/skills/nano-banana-2/scripts/generate_image.py`](https://github.com/intellectronica/agent-skills/blob/main/skills/nano-banana-2/scripts/generate_image.py).

### Invocation

Always run from the deck root (`.octocode/slides/{{slideName}}/`) so the image lands in `assets/` next to where the slide references it:

```bash
cd .octocode/slides/{{slideName}}
uv run /absolute/path/to/skills/octocode-slides/scripts/generate_image.py \
  --prompt "<prompt>" \
  --filename "assets/{{slug}}-{{timestamp}}.png" \
  --resolution 2K \
  --aspect-ratio 16:9
```

### Flags

| Flag | Default | Slide guidance |
|------|---------|---------------|
| `--prompt` | required | Use the Slide Image Prompt template below |
| `--filename` | required | `assets/{{slug}}-yyyy-mm-dd-hh-mm-ss.png` — slug matches the slide file |
| `--resolution` | `1K` | `1K` for inline / decorative · `2K` for `slide--two-col` half · `4K` only for full-bleed `slide--image` |
| `--aspect-ratio` | model default | `16:9` matches the 1280×720 slide canvas · `1:1` for inline tiles · `2:3` / `3:4` for portrait insets |
| `--input-image` | none | Repeat up to 14 times for character/object consistency across the deck |
| `--api-key` | `GEMINI_API_KEY` env | Use the env var; pass the flag only when a per-deck key is required |

### Environment

```bash
export GEMINI_API_KEY="..."        # get from https://aistudio.google.com/apikey
command -v uv >/dev/null            # required
```

If `uv` is not installed: `brew install uv` (macOS) or `curl -LsSf https://astral.sh/uv/install.sh | sh`.

---

## Path B — Third-party CLI (`belt`)

Only when the user explicitly prefers inference.sh, or the deck needs Google Search grounding (real-time facts in images — weather, news, current events):

```bash
belt login
belt app run google/gemini-3-1-flash-image-preview --input '{
  "prompt": "<prompt>",
  "aspect_ratio": "16:9",
  "resolution": "2K",
  "enable_google_search": true,
  "images": ["assets/reference.png"]
}'
```

Save the returned image into `assets/{{slug}}-{{timestamp}}.png` manually (the CLI prints a URL/path; download with `curl`).

---

## Path C — Gemini CLI + `mcp-nanobanana-go` (official Google MCP)

Use when the user is already working inside Gemini CLI and has a Google Cloud project.

### How it works

[`mcp-nanobanana-go`](https://github.com/GoogleCloudPlatform/vertex-ai-creative-studio/tree/main/experiments/mcp-genmedia/mcp-genmedia-go/mcp-nanobanana-go) is an official Google MCP server that exposes `nanobanana_image_generation` as a tool. When configured in Gemini CLI, the agent can call it directly during a session — no Python, no `uv`.

### Setup

**1. Install the MCP server binary:**

```bash
curl -sL https://raw.githubusercontent.com/GoogleCloudPlatform/vertex-ai-creative-studio/main/experiments/mcp-genmedia/mcp-genmedia-go/install-online.sh | bash
# Adds mcp-nanobanana-go to ~/.local/bin — ensure that is on $PATH
```

**2. Set required env vars:**

```bash
export GOOGLE_CLOUD_PROJECT="your-gcp-project-id"
export GOOGLE_CLOUD_LOCATION="us-central1"   # optional, this is the default
gcloud auth application-default login         # once — sets up ADC
```

**3. Configure Gemini CLI** (`~/.gemini/settings.json`):

```json
{
  "mcpServers": {
    "nanobanana": {
      "command": "mcp-nanobanana-go",
      "args": [],
      "env": {
        "GOOGLE_CLOUD_PROJECT": "${GOOGLE_CLOUD_PROJECT}",
        "GOOGLE_CLOUD_LOCATION": "${GOOGLE_CLOUD_LOCATION}"
      }
    }
  }
}
```

**4. Install the image-artist skill** (optional but recommended — teaches Gemini CLI prompting best practices):

```bash
cp -r /path/to/vertex-ai-creative-studio/experiments/mcp-genmedia/skills/genmedia-image-artist \
      ~/.gemini/skills/
```

### Invocation

Inside a Gemini CLI session, just describe what you want. The agent calls `nanobanana_image_generation` automatically:

```text
> Generate a cinematic hero image of a futuristic city at dusk, 16:9, for the title slide
```

Output goes to the `GENMEDIA_BUCKET` GCS bucket, or you can specify a local path in the prompt.

### Env vars for Path C

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `GOOGLE_CLOUD_PROJECT` | Yes | — | Your GCP project ID |
| `GOOGLE_CLOUD_LOCATION` | No | `us-central1` | Vertex AI region |
| `GENMEDIA_BUCKET` | No | — | Default GCS output bucket |
| `ALLOW_UNSAFE_MODELS` | No | `false` | Bypass model validation for pre-release strings |

Auth uses [Application Default Credentials](https://cloud.google.com/docs/authentication/application-default-credentials) — **not** `GEMINI_API_KEY`. ADC resolves in this order: `gcloud auth application-default login` → service account key file (`GOOGLE_APPLICATION_CREDENTIALS`).

### Path C vs Path A — when to choose

| | Path A (Python SDK) | Path C (Gemini CLI MCP) |
|-|---------------------|------------------------|
| Auth | `GEMINI_API_KEY` from AI Studio | GCP ADC + project |
| Control | Full programmatic control | Agent-driven, conversational |
| Scripting | `uv run generate_image.py …` | Gemini CLI session |
| Billing | AI Studio quota | Vertex AI quota |
| Reference images | Up to 14 via `--input-image` | Via `images` param in tool call |
| Best for | Automated slide builds, CI | Interactive sessions, rapid iteration |

---

## Slide Image Prompt template

The slide's `data-expected` description is too thin to send to the model directly. Expand it through this template — same shape as `references/04-design.md`'s design tokens, so generated images stay consistent with the theme:

```
Create a {{layout}} image of: {{subject}}.
Style: {{style}} — anchored to the deck's visual system ({{primary_color}}, {{accent_color}}, {{mood}}).
Composition: {{shot}} ({{focal_point}} dominant, {{rule_of_thirds | centered | rule_of_fifths}}).
Lighting: {{lighting}}.
Background: {{background}} — must work behind {{light | dark}} caption overlay.
Negative space: leave {{top | bottom | left | right}} {{30%}} clear for text.
Avoid: text, watermarks, logos, hands with malformed fingers, signature artifacts, clichéd stock-photo poses.
```

For full-bleed `slide--image` slides, the `.image-overlay` gradient sits on top — generate **without** built-in text; let the slide system render the caption.

### Style anchors (pick one per deck, write into `DESIGN.md`)

| Style | When |
|-------|------|
| `photorealistic, editorial photography, 50mm` | Executive / business decks, real-world subjects |
| `clean vector illustration, flat color, geometric` | Product / SaaS, technical concepts |
| `3D render, soft studio lighting, isometric` | Architecture, infrastructure diagrams as hero |
| `watercolor / hand-drawn` | Storytelling decks, education, non-technical audiences |
| `cinematic, anamorphic, shallow depth of field` | Mood / opener / closer slides |

Pick **one** style for the whole deck. Mixing styles is visual slop — counts against Visual Slop Test signal #5 (palette inconsistency).

---

## Resolution × slide-type matrix

| Slide type | Resolution | Aspect ratio | Why |
|------------|-----------|--------------|-----|
| `slide--image` (full-bleed) | `4K` | `16:9` | Edges visible at presentation zoom |
| `slide--two-col` (image side) | `2K` | `4:5` or `1:1` | Half the canvas — 2K is the sweet spot |
| Inline tile in `content` / `stats` | `1K` | `1:1` | Decorative, small |
| Portrait inset (speaker, product) | `2K` | `2:3` or `3:4` | Vertical composition |
| Thumbnail / draft pass | `512px` | match final | Iterate prompt cheaply, then re-run at target res |

**Workflow:** draft at `512px` until the prompt is right, then re-run at the target resolution. Don't burn 4K calls on prompt iteration.

---

## Character / object consistency across slides

When the deck reuses a subject (mascot, product, recurring person) on multiple slides, use the **reference-image** flag so the model sees the same subject every time:

```bash
uv run generate_image.py \
  --prompt "<scene description>. Keep subject identity from reference image." \
  --filename "assets/scene-2.png" \
  --input-image "assets/scene-1.png" \
  --resolution 2K
```

Up to **14** reference images per call. For a campaign:
- pass the product render once
- pass the brand mark once
- pass any prior generated scenes

This is the only reliable way to stop the model from drifting between slides.

---

## Asset save location (path contract)

Generated images live in **the deck's `assets/` folder**, never in the skill folder:

```
.octocode/slides/{{slideName}}/
├── assets/
│   ├── {{slug}}-yyyy-mm-dd-hh-mm-ss.png      ← generated
│   ├── {{slug}}-yyyy-mm-dd-hh-mm-ss-1k.png   ← draft (delete after final)
│   └── user-provided.png                      ← untouched
```

Slides reference them with the existing one-level-up pattern: `../assets/{{filename}}`. No change to the path contract in `SKILL.md → Output structure`.

---

## Integration with existing image-handling rule

`references/05-implementation.md` currently says:

> *For any missing image, do not search, download, generate, or silently substitute an image. Render the `PLACEHOLDER` component...*

This still holds as the default. **Generation is opt-in.** The decision table becomes:

| Image status in brief | What to do in HTML |
|-----------------------|--------------------|
| `ready` — file path provided | `<img src="../assets/{{filename}}">` |
| `placeholder` — user will provide later | `image-ph` / `image-ph-bleed` component |
| `generate` — user opted in **and** `data-expected` is concrete | Run `generate_image.py` → save to `assets/` → `<img>` it. Keep the `data-expected` on the element as a comment for traceability. |
| anything ambiguous | Treat as `placeholder` |

The `data-expected` attribute is what becomes (after expansion via the Slide Image Prompt template) the `--prompt` to the helper.

---

## Phase 6 review checks for generated images

Add these to the Slop / handoff pass when any image was generated:

- [ ] **Style consistency** — every generated image uses the same `Style anchor` from `DESIGN.md`. Mixed styles = automatic Visual Slop hit.
- [ ] **No hallucinated text** — model-rendered text on the image (sign, billboard, UI mock) is checked against the deck's claim. If text doesn't match, regenerate with `Avoid: text, signs, UI text` and add real text in HTML overlay.
- [ ] **Hands, faces, fingers** — Nano Banana 2 is good but not perfect. Spot-check at 4K; if a hand is malformed, regenerate or crop.
- [ ] **No watermark / signature** — sometimes the model adds a fake signature corner; reroll if present.
- [ ] **Caption legibility** — for full-bleed images with overlay text, verify the overlay gradient still reads against the generated image. Adjust gradient stops in slide CSS if needed.
- [ ] **Provenance note** — every generated image is marked in `request.md → Images` with model code, prompt, and timestamp. Counts as Content Slop signal #3 (uncited source) if missing.

---

## Cost & rate-limit notes

| Resolution | Approx. credits per image | Notes |
|-----------|--------------------------|-------|
| `512px` | lowest | Iterate freely |
| `1K` | low | Default |
| `2K` | ~2× 1K | Most slide images |
| `4K` | ~4× 1K | Reserve for full-bleed final |

The Gemini 3.1 Flash Image Preview tier is rate-limited per minute and per day. For a 20-slide deck with 6 images, expect ~10–12 API calls (drafts + finals). Don't generate during Phase 4 previews — wait for the design direction to lock.

---

## Quick recipes

**Full-bleed hero, photorealistic:**
```bash
uv run scripts/generate_image.py \
  --prompt "Wide cinematic shot: empty modern office at blue hour, desks lit only by laptop screens, deep shadows, anamorphic lens flare. Negative space on right 40% for caption overlay. Avoid: people, text, signage." \
  --filename "assets/title-2026-05-10-22-14-00.png" \
  --resolution 4K --aspect-ratio 16:9
```

**Two-column product render, vector style:**
```bash
uv run scripts/generate_image.py \
  --prompt "Clean isometric vector illustration of a server rack, flat colors matching #0EA5E9 primary and #F59E0B accent, white background, no text, no shadows beyond soft drop." \
  --filename "assets/architecture-2026-05-10-22-15-30.png" \
  --resolution 2K --aspect-ratio 4:5
```

**Iterate on a draft until prompt is right:**
```bash
for variant in "morning fog" "blue hour" "neon rain"; do
  uv run scripts/generate_image.py \
    --prompt "Tokyo street, $variant, cinematic, no text" \
    --filename "assets/draft-${variant// /-}.png" \
    --resolution 512px --aspect-ratio 16:9
done
# Then re-run the winner at 4K with the same prompt.
```

---

## When to escalate to Nano Banana Pro

Swap `--model gemini-3-pro-image-preview` (and adjust the helper accordingly) when:

- The image must be **factually accurate** (real building, real product, branded element). Pro has stronger world knowledge.
- The slide is the **deck's one surprise moment** (see SKILL.md → Storytelling → "One surprise per deck"). Pay the quality premium once.
- The audience is **Executive** depth — fewer slides, each carrying more weight.

Otherwise stay on Flash Image (Nano Banana 2) — the speed difference matters more during iteration than the quality delta in slide-scale viewing.

---

## Env Params Best Practice — How to Pass API Keys to Skills

This section documents the canonical pattern for declaring and resolving environment parameters in any skill that calls external APIs.

### Resolution order (two-layer pattern)

The `intellectronica/agent-skills` nano-banana reference implementation defines the authoritative precedence:

1. `--api-key` CLI argument — use when the user provides a key inline during the chat session, or when a per-deck override is required.
2. `GEMINI_API_KEY` environment variable — the ambient default; set once in the shell profile and forget.
3. If neither is found → **exit loudly** with clear instructions for both options.

```python
def get_api_key(provided_key: str | None) -> str | None:
    if provided_key:
        return provided_key
    return os.environ.get("GEMINI_API_KEY")

api_key = get_api_key(args.api_key)
if not api_key:
    print("Error: No API key provided.", file=sys.stderr)
    print("  1. Provide --api-key argument", file=sys.stderr)
    print("  2. Set GEMINI_API_KEY environment variable", file=sys.stderr)
    sys.exit(1)
```

### How to document env vars in a SKILL.md

Declare env requirements in a `## Prerequisites` section at the top, before any workflow steps. Use a table — agents scan tables faster than prose.

**`## Prerequisites`**

**`### Environment`**

| Variable | Required | How to get it | Setup |
|----------|----------|---------------|-------|
| `GEMINI_API_KEY` | Yes (Path A/B) | [AI Studio](https://aistudio.google.com/apikey) | `export GEMINI_API_KEY="..."` |
| `GOOGLE_CLOUD_PROJECT` | Yes (Path C) | GCP Console | `export GOOGLE_CLOUD_PROJECT="my-project"` |
| `GOOGLE_CLOUD_LOCATION` | No (Path C) | — | Defaults to `us-central1` |

**`### Setup check`**

```bash
# Path A
echo $GEMINI_API_KEY && command -v uv
# Path C
gcloud auth application-default print-access-token && echo $GOOGLE_CLOUD_PROJECT
```

### Rules

- **Never hardcode keys** in SKILL.md examples. Use `"..."` or `"your-key-here"` as placeholders.
- **Never commit `.env` files** containing real keys.
- **Prefer env vars over CLI args** for ambient credentials. Reserve `--api-key` for explicit per-invocation overrides.
- **Fail loudly** — a missing key should print the exact export command the user needs, not a generic error.
- **Document the source** — link to where the user gets the key (AI Studio, GCP Console, etc.).
- **For GCP tools** — use ADC (`gcloud auth application-default login`), not service account keys, for local development. Document the command explicitly.

---

## References

### Path A — Direct API
- **Reference helper script:** [`intellectronica/agent-skills/skills/nano-banana-2/scripts/generate_image.py`](https://github.com/intellectronica/agent-skills/blob/main/skills/nano-banana-2/scripts/generate_image.py)
- **Nano Banana 2 SKILL.md (canonical API key pattern):** [`intellectronica/agent-skills/skills/nano-banana-2/SKILL.md`](https://github.com/intellectronica/agent-skills/blob/main/skills/nano-banana-2/SKILL.md)
- **Official model docs:** [ai.google.dev/gemini-api/docs/image-generation](https://ai.google.dev/gemini-api/docs/image-generation)
- **API key:** [aistudio.google.com/apikey](https://aistudio.google.com/apikey)

### Path B — Third-party CLI
- **`belt` CLI:** [`inference-sh/skills/tools/image/nano-banana-2`](https://github.com/inference-sh/skills/tree/main/tools/image/nano-banana-2)
- **Grounded generation pipeline:** [`GeeveGeorge/openNanoBanana`](https://github.com/GeeveGeorge/openNanoBanana)

### Path C — Gemini CLI MCP
- **`mcp-nanobanana-go` server:** [`GoogleCloudPlatform/vertex-ai-creative-studio/experiments/mcp-genmedia/mcp-genmedia-go/mcp-nanobanana-go`](https://github.com/GoogleCloudPlatform/vertex-ai-creative-studio/tree/main/experiments/mcp-genmedia/mcp-genmedia-go/mcp-nanobanana-go)
- **Easy installer:** [`install-online.sh`](https://github.com/GoogleCloudPlatform/vertex-ai-creative-studio/blob/main/experiments/mcp-genmedia/mcp-genmedia-go/install-online.sh)
- **`genmedia-image-artist` skill:** [`GoogleCloudPlatform/vertex-ai-creative-studio/experiments/mcp-genmedia/skills/genmedia-image-artist/SKILL.md`](https://github.com/GoogleCloudPlatform/vertex-ai-creative-studio/blob/main/experiments/mcp-genmedia/skills/genmedia-image-artist/SKILL.md)
- **ENV_VARS reference:** [`mcp-genmedia-go/ENV_VARS.md`](https://github.com/GoogleCloudPlatform/vertex-ai-creative-studio/blob/main/experiments/mcp-genmedia/mcp-genmedia-go/ENV_VARS.md)
- **Gemini CLI repo:** [`google-gemini/gemini-cli`](https://github.com/google-gemini/gemini-cli)

### Shared / Pro
- **Alternative skill (Pro variant):** [`steipete/agent-scripts/skills/nano-banana-pro`](https://github.com/steipete/agent-scripts/tree/main/skills/nano-banana-pro)
