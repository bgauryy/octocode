# Phase 1 — Request

**Role:** Intake agent. Understand the request fully before any research begins.

**Input:** User conversation
**Output:** `.content/request.md`

---

## Step 1 · Read the request and infer what you can

Read the user's message carefully. Extract every piece of information already given:
- Topic / title
- Audience (who, expertise, posture)
- Goal (teach / pitch / update / inspire / demo / workshop)
- Source files or material provided
- Aesthetic / brand preference
- Slide count or time constraint
- Any images or diagrams mentioned

**Rule:** If a field can be inferred confidently from context, infer it and record the assumption — do not ask. Ask only for what is genuinely unknown and would change the deck if you guessed wrong.

---

## Step 2 · Ask what is missing (one focused question)

**Run the Think-before-asking protocol first** (`SKILL.md → Think before asking`). Complete Steps 1–3 of that protocol internally before writing any question. Only ask if genuinely unknown fields remain after inference.

**If the user's request is complete enough to proceed:** skip this step entirely. Write `request.md` and continue.

**If fields remain genuinely unknown:** bundle all into one message using `SKILL.md → Presenting options to the user`. Show what you already know before asking:

```
Got it — starting slides on "{{topic}}".

Inferred: Audience: {{}} · Goal: {{}} · Depth: {{}} · Aesthetic: {{or "your call"}}

One thing I need before starting:
{{Include ONLY lines below that are genuinely unknown — remove the rest}}

Source material:
  A — Folder path (I'll read it)
  B — Specific files (paste paths)
  C — No files — I'll work from the topic description

Audience (if unclear):
  A — {{most likely option inferred from context}}
  B — {{second option}}
  C — Other: describe

Slide count:
  A — Exec brief (5–10)
  B — Pitch / update (10–15)
  C — Technical deep-dive (15–30)
  D — Your call

Aesthetic:
  A — Describe a vibe (dark/light, bold/minimal, technical/editorial…)
  B — Brand guide — paste path or colors/fonts
  C — No preference — your call

Reply with letters (e.g. "A, C, A") or correct anything above.
```

**Smart rules for asking:**
- If the topic is obvious from the message, do NOT ask "what is the topic?"
- If the audience is obvious (e.g., "for my engineering team"), do NOT ask about audience
- If the user says "your call", "just build it", or "fast mode", skip this step and proceed with assumptions
- Never ask more than one question per unknown — pick the most important if several are missing and you can infer the rest

---

## Step 3 · Read source files

If source file paths were given (in initial message or from Step 2 answer), read them now. Choose tools based on what was provided:

| Source type | Tool routing |
|-------------|-------------|
| Local folder path | `localViewStructure` → identify key files → `localSearchCode` for key concepts → `localGetFileContent` for relevant sections |
| Local file path(s) | `localGetFileContent` on each; read in parallel |
| GitHub repo URL | `ghViewRepoStructure` → `ghSearchCode` for key patterns → `ghGetFileContent` for relevant files |
| GitHub PR / commit | `ghSearchPRs` for context, then dive into changed files |
| npm / pip / other package | `npmSearch` first to get repo URL, then treat as GitHub repo |
| No path — description only | Skip this step; record as "Source: user description" in `request.md` |

Read in parallel when possible. For any repo: view structure first, then search key concepts, then read 3–5 files most relevant to the deck topic. For code: extract architecture decisions, key APIs, real examples — not implementation minutiae.

If a brand guide was given: record exact values — hex colors, font names, spacing rules. Mark the brief `brand_guide: locked`.

---

## Step 4 · Write request.md

Derive `slideName` as a lowercase kebab-slug of the deck title (e.g. "API Caching Deep Dive" → `api-caching-deep-dive`). Create `.content/request.md` inside `.octocode/slides/{{slideName}}/`. This single file is the source of truth for what the user wants and what was gathered. Research findings (Phase 2) will be appended to this same file.

```markdown
# Request: {{Title}}

## What the user wants
- **Topic:** {{}}
- **Audience:** {{who}} · {{expertise: expert / practitioner / informed / general}} · {{posture: skeptical / neutral / bought-in}}
- **Depth level:** {{Executive / Management / Technical / Mixed / Async}} ← inferred from audience + goal
- **Goal:** {{teach / pitch / update / inspire / demo}}
- **Slide count:** {{target or range}}
- **Tone / aesthetic:** {{description or "not specified — your call"}}
- **Brand guide:** {{path or values, or "none"}}

## Assumptions made
{{List any field that was inferred, not stated. If none: "None — all fields confirmed by user."}}

## Source files
| Path | Summary |
|------|---------|
| {{path}} | {{one-line description of relevant content}} |

## Raw content notes
{{Key facts, quotes, code, data — exactly as found. No interpretation. Be brief.}}

## Images
{{If no images mentioned: "None."}}
| Purpose | Path or description | Status |
|---------|---------------------|--------|
| {{e.g. title hero, product screenshot}} | {{path or "user will provide"}} | {{ready / placeholder}} |

## Known gaps
{{What is still needed: stats, code, comparisons, images, etc.}}
{{If none: "None — source material is sufficient."}}

---
<!-- Phase 2 research findings will be appended below this line -->
```

---

## Gate 1 — Smart stop

**Show the user only when confirmation adds real value.** In most cases: write `request.md` and move directly to Phase 2.

Ask the user to confirm when:
- The topic is ambiguous and the wrong interpretation would waste all of Phase 2
- Source files were listed but couldn't be read (access error, missing path)

When asking:
```
Request captured.

Topic: {{title}}
Audience: {{}} · Goal: {{}} · Depth: {{}}
Source files read: {{n or "none"}}
Assumptions: {{list or "none"}}
Gaps: {{list or "none"}}

Reply "good" to start research, or correct anything above.
```

If the request is clear: send a one-line progress note ("Request captured — starting research") and continue.
