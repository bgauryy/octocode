# Report Data Schema

The HTML template uses per-section `<script>` blocks (`__REPORT_META__`, `__SECTION_AI__`, `__SECTION_DEVTOOLS__`, etc.). The client JS assembles the full report from these pieces.

## Build Pipeline

### Section-dir mode (parallel workflow — preferred)

```
sections/meta.json + sections/{id}.json → build-report --section-dir → validated.json + final.html
```

1. Each subagent writes `{id}.json` for its domain (follows `SectionPayload` schema).
2. Coordinator writes `meta.json` (follows `ReportMeta` schema).
3. Run `build-report --section-dir` — merges, validates, normalizes, injects per-section, opens browser.

### Legacy single-file mode

```
raw.json → build-report --input → validated.json + final.html
```

1. Write a full report object (`raw.json`) with all sections inline.
2. Run `build-report --input` — validates, normalizes, splits into per-section blocks, opens browser.

Source lives in `src/`. Built artifacts live in `scripts/`. CSS is inlined at build time via `__INLINE_CSS__`.

## Zod Schema (source of truth: `src/report-schema.ts`)

```typescript
const DomainId = z.enum(["ai", "devtools", "web", "security", "repos", "cross"]);
const ItemType = z.enum(["release", "advisory", "blog", "changelog", "newsletter", "repo"]);

const ContentEvidence = z.object({
  method: z.literal("full-page"),
  chars: z.number().int().min(200),
  fetchedAt: z.string().datetime({ offset: true }).optional()
});

const Reference = z.union([
  z.string().url(),
  z.object({
    label: z.string().min(1).optional(),
    title: z.string().min(1).optional(),
    source: z.string().min(1).optional(),
    url: z.string().url(),
    kind: z.string().min(1).optional()
  }).passthrough()
]);

const Item = z.object({
  domain: DomainId,
  type: ItemType,
  title: z.string().min(1),
  shortTitle: z.string().min(1).optional(),
  link: z.string().url(),
  source: z.string().min(1),
  sourceUrl: z.string().url().optional(),
  summary: z.string().min(1),
  shortDescription: z.string().min(1).optional(),
  whyImportant: z.string().min(1).optional(),   // at least one of whyImportant/whyInteresting required
  whyInteresting: z.string().min(1).optional(),
  references: z.array(Reference).default([]),
  date: z.string().min(1),
  dateISO: z.string().date(),            // YYYY-MM-DD
  heat: z.number().int().min(0).max(100),
  layer: z.number().int().min(0).max(4).optional(),
  stars: z.string().optional(),
  imageUrl: z.string().url().optional(),
  contentEvidence: ContentEvidence
}).passthrough();

const Section = z.object({
  id: DomainId,
  name: z.string().min(1),
  icon: z.string().min(1),
  iconClass: z.enum(["si-ai","si-dt","si-web","si-sec","si-repo","si-cross","si-neutral"]),
  quiet: z.boolean(),
  quietMsg: z.string().optional(),
  items: z.array(Item).default([])
}).passthrough();

const SourceCheck = z.object({
  source: z.string().min(1),
  status: z.string().min(1),
  found: z.union([z.boolean(), z.number().int().min(0), z.string().min(1)]),
  notes: z.string().default(""),
  url: z.string().url().optional(),
  domain: DomainId.optional(),
  category: z.string().min(1).optional()
}).passthrough();

const ReportData = z.object({
  window: z.string().min(1),             // "Mar 28-Apr 3, 2026"
  windowLabel: z.enum(["24h","7d","14d","30d"]),
  generated: z.string().date(),          // YYYY-MM-DD
  tldr: z.string().min(120),
  topItems: z.array(Item).min(3).max(30),
  sections: z.array(Section).min(5).max(6),
  sourcesChecked: z.array(SourceCheck).default([])
}).passthrough();
```

## Per-Section Schemas (source of truth: `src/report-schema.ts`)

### SectionPayload (written by each subagent)

```typescript
const SectionPayload = z.object({
  id: DomainId,
  name: z.string().min(1),
  icon: z.string().min(1),
  iconClass: IconClass,
  quiet: z.boolean(),
  quietMsg: z.string().optional(),
  items: z.array(Item).default([]),
  sourcesChecked: z.array(SourceCheck).default([])
}).passthrough();
```

### ReportMeta (written by coordinator)

```typescript
const ReportMeta = z.object({
  window: z.string().min(1),
  windowLabel: z.enum(["24h", "7d", "14d", "30d"]),
  generated: z.string().date(),
  tldr: z.string().min(120),
  topItems: z.array(Item).min(3).max(30),
  sourcesChecked: z.array(SourceCheck).default([])
}).passthrough();
```

### Section directory structure

```
~/tmp/{ts}-sections/
  meta.json          ← coordinator writes (ReportMeta)
  ai.json            ← subagent writes (SectionPayload)
  devtools.json      ← subagent writes (SectionPayload)
  web.json           ← subagent writes (SectionPayload)
  security.json      ← subagent writes (SectionPayload)
  repos.json         ← subagent writes (SectionPayload)
  cross.json         ← coordinator writes if needed (SectionPayload, optional)
```

`build-report --section-dir` merges all files, unions `sourcesChecked`, validates the merged report, then injects each section into its own `<script>` block in the HTML.

## Constraints

| Rule | Detail |
|------|--------|
| `topItems` exclusivity | Must not also appear in `sections[].items` |
| `heat` ordering | Template sorts by `heat` desc, then `dateISO` desc |
| `contentEvidence` | Required: `method: "full-page"`, `chars >= 200` |
| `whyImportant` | Required (or legacy `whyInteresting`) on every item |
| `references` | At least one per item |
| Section order | Always include `ai`, `devtools`, `web`, `security`, `repos`; optional `cross` |
| `quiet` sections | `quiet: true` + `quietMsg` when a domain has no items |
| `sourcesChecked` | Audit trail — every checked/blocked/stale/skipped source |
| Total items | 3–300 across `topItems` + all sections |

## What the Template Renders

The HTML shows: hero with stats, highlights panel, domain section cards, item cards with source links and reference menus. Items display title, summary, whyImportant, heat, date, domain, type, source, and references.

Fields the agent must provide for good rendering:
- `title`, `summary`, `whyImportant` — the editorial copy
- `shortTitle`, `shortDescription` — compact variants for low-heat items
- `references` — at least one; multi-ref items get a dropdown menu
- `heat` — drives sort order and highlight selection (>=70 = high-signal)
- `source`, `sourceUrl` — attribution link in each card
- `imageUrl` — optional hero image

## Example

```json
{
  "window": "Mar 28-Apr 3, 2026",
  "windowLabel": "7d",
  "generated": "2026-04-03",
  "tldr": "AI launch velocity stayed high while tooling improved operator workflows and the repo layer produced practical signals for engineers tracking platform change.",
  "topItems": [
    {
      "domain": "ai",
      "type": "blog",
      "title": "OpenAI expands enterprise AI controls for production teams",
      "link": "https://example.com/ai-1",
      "source": "OpenAI",
      "summary": "OpenAI published new enterprise controls for managed AI deployments focusing on governance, rollout control, and operational safety for larger teams.",
      "whyImportant": "Shifts the baseline for how enterprises govern production AI systems.",
      "references": [{ "label": "OpenAI News", "url": "https://example.com/ai-1", "kind": "primary" }],
      "date": "Apr 3",
      "dateISO": "2026-04-03",
      "heat": 92,
      "contentEvidence": { "method": "full-page", "chars": 864 }
    }
  ],
  "sections": [
    { "id": "ai", "name": "AI", "icon": "A", "iconClass": "si-ai", "quiet": false, "items": [] },
    { "id": "devtools", "name": "Devtools", "icon": "D", "iconClass": "si-dt", "quiet": false, "items": [] },
    { "id": "web", "name": "Web Platform", "icon": "W", "iconClass": "si-web", "quiet": true, "quietMsg": "No notable web platform changes.", "items": [] },
    { "id": "security", "name": "Security", "icon": "S", "iconClass": "si-sec", "quiet": false, "items": [] },
    { "id": "repos", "name": "Repos", "icon": "R", "iconClass": "si-repo", "quiet": true, "quietMsg": "No notable repo shifts.", "items": [] }
  ],
  "sourcesChecked": [
    { "source": "OpenAI News", "status": "checked", "found": 1, "url": "https://openai.com/news/", "domain": "ai" }
  ]
}
```

## Serialization

Each block: `JSON.stringify(data).replace(/</g, "\\u003c")`.

The HTML template has per-section placeholders:
- `__REPORT_META__` — meta block (window, tldr, topItems, reportMeta)
- `__SECTION_AI__`, `__SECTION_DEVTOOLS__`, `__SECTION_WEB__`, `__SECTION_SECURITY__`, `__SECTION_REPOS__` — required sections
- `__SECTION_CROSS__` — optional (inject `null` if absent)

The client JS reads each `<script>` block by element ID, parses them, and assembles the full report object before rendering.
