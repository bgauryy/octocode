# Phase 1 — Brief

**Role:** Intake agent. Understand the request fully before any research begins.

**Input:** User conversation
**Output:** `.content/brief.md`

---

## Step 1 · Resolve the brief with minimum friction

First read the user's request and infer what you can. If the topic, audience, goal, source material, and output format are clear enough to proceed, do **not** ask a question. Write the assumptions into `brief.md` and continue.

Ask one bundled question only when a missing answer would materially change the deck. Include only the unknown fields; do not ask the user to repeat information already provided.

```
Tell me about the deck:

1. Do you have source materials?
   (a) Folder path — I'll read it with local tools
   (b) Specific files
   (c) No files — describe the content here

2. Topic / title:
3. Audience:
   (a) Who are they? (devs / execs / mixed / students / customers / investors)
   (b) Expertise level? (expert · practitioner · informed · general)
   (c) Posture? (skeptical · neutral · already bought in)
4. Goal (teach / pitch / update / inspire / demo):
5. Approx slide count (5–10 exec brief / 10–15 pitch / 15–30 technical deep-dive / your call):
6. Any tone or aesthetic preference?
   (a) Describe a vibe / aesthetic
   (b) I have a brand guide — share the path or paste colors/fonts
   (c) No preference — your call

7. Do you have images, screenshots, or diagrams to include?
   (a) Yes — folder path: _______
   (b) Yes — specific file paths: _______
   (c) I'll describe them and drop in the paths later
   (d) No images — text and data only
```

---

## Step 2 · Read source files and brand guide (if paths were given)

Read in parallel using Octocode local tools when available:

```
View source folder structure
Find relevant source files (`.md`, `.txt`, `.html`, `.pdf`, `.pptx`, code files)
Read the 3–5 most relevant files first
```

Fallback if Octocode local tools are unavailable: use `rg --files`, `find`, `sed`, `head`, `pdftotext`, or other local readers appropriate to the file type.

**If a brand guide path was given (question 6b):** read it now and record exact values — hex colors, font names, spacing rules. Mark the brief with `brand_guide: locked`. Phase 4 should treat it as the primary design source and skip visual research unless the guide is incomplete or the user asks for exploration.

Record only what the deck may use: key facts, quotes, code, data, brand values, image paths, and obvious gaps. Avoid pasting long documents into the brief — Phase 2 handles deeper synthesis.

---

## Step 3 · Write brief.md

Create `.content/brief.md` inside `.octocode/slides/{{slideName}}/`. Keep it concise enough that the next phase can scan it quickly.

```markdown
# Brief: {{Title}}

## Deck intent
- **Audience:** {{who}} · **Expertise:** {{expert / practitioner / informed / general}} · **Posture:** {{skeptical / neutral / bought-in}}
- **Depth level:** {{Executive / Management / Technical / Mixed / Async}} ← inferred from audience + goal
- **Goal:** {{}}
- **Slide count:** {{}}
- **Tone / aesthetic:** {{or "not specified"}}
- **Brand guide:** {{path or values, or "none"}}

## Source files read
| Path | One-line summary |
|------|-----------------|
| {{path}} | {{}} |

## Raw content notes
{{Key points, quotes, code, data — exactly as found. No interpretation.}}

## Images inventory
| Slide purpose | File path or description | Status |
|---------------|--------------------------|--------|
| {{e.g., "hero title background"}} | {{path or "user will provide"}} | {{ready / placeholder}} |

## Known gaps
{{What we still need to find: stats, code examples, context, comparisons, etc.}}
{{If none: "None — source files are sufficient."}}
```

---

## Gate 1 — Smart stop

Show the user only if confirmation is needed. If the user delegated judgment or the request is clear, send a short progress update and continue to Phase 2.

```
Brief captured.

Topic: {{title}}
Audience: {{}} · Goal: {{}}
Source files read: {{n or "none"}}
Images: {{n ready · n placeholder · or "none"}}
Gaps for research: {{list or "none"}}

Reply "good" to start research, or correct anything above.
```

Stop here only if the brief has a real blocker. Otherwise continue with stated assumptions.
