# Phase 1 — Request

**Role:** Intake agent. Understand what the user wants and gather any source material before research begins.

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

**If the user's request is complete enough to proceed:** skip this step entirely. Write `request.md` and continue.

**If one or more fields are ambiguous or missing:** ask one bundled question. Include only the unknown fields — do not ask for information the user already gave.

```
A few things to confirm before I start:

{{Include only the lines below that are actually unknown}}

Source material:
  (a) Folder path — I'll read it
  (b) Specific files — paste the paths
  (c) No files — describe the content here

Audience:
  Who are they? (devs / execs / mixed / students / customers / investors)
  Expertise level? (expert / practitioner / informed / general)

Goal: (teach / pitch / update / inspire / demo)

Slide count:
  (a) Exec brief — 5–10
  (b) Pitch / update — 10–15
  (c) Technical deep-dive — 15–30
  (d) Your call

Aesthetic / brand:
  (a) Describe a vibe (dark/light, bold/minimal, technical/editorial…)
  (b) I have a brand guide — paste the path or colors/fonts
  (c) No preference — your call

Images to include?
  (a) Yes — folder path or file list
  (b) I'll add them later
  (c) No images needed
```

**Smart rules for asking:**
- If the topic is obvious from the message, do NOT ask "what is the topic?"
- If the audience is obvious (e.g., "for my engineering team"), do NOT ask about audience
- If the user says "your call", "just build it", or "fast mode", skip this step and proceed with assumptions
- Never ask more than one question per unknown — pick the most important if several are missing and you can infer the rest

---

## Step 3 · Read source files

If source file paths were given (in initial message or from Step 2 answer), read them now using available local tools.

Read in parallel when possible:
- View folder structure if a directory was given
- Read the 3–5 most relevant files first (`.md`, `.txt`, `.html`, code files)
- For repos: view structure, search key concepts, read key files

If a brand guide was given: record exact values — hex colors, font names, spacing rules. Mark the brief `brand_guide: locked`.

---

## Step 4 · Write request.md

Create `.content/request.md` inside `.octocode/slides/{{slideName}}/`. This single file is the source of truth for what the user wants and what was gathered. Research findings (Phase 2) will be appended to this same file.

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
| Purpose | Path or description | Status |
|---------|---------------------|--------|
| {{hero background}} | {{path or "user will provide"}} | {{ready / placeholder}} |

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
- The user's aesthetic preference is highly specific and you need confirmation before Phase 4

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
