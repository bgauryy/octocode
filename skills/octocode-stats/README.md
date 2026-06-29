# Octocode Stats

`octocode-stats` builds a local HTML dashboard from Octocode usage stats. It is for users who want to understand response savings, cache behavior, errors, and rate-limit effects without manually reading `stats.json`.

The dashboard builder script owns parsing, calculations, rendering, and optional browser opening.

## When to use

- Show Octocode usage stats.
- Render a local dashboard.
- Estimate tokens or characters saved by Octocode responses.
- Inspect cache hits, avoided rate limits, errors, or registry failures.
- Render a specific stats file from a test run or alternate `OCTOCODE_HOME`.

Use `octocode` or `octocode-research` for code research. Use `octocode-awareness` for durable memory or handoff data. Use setup guidance, not this skill, for credentials or MCP install questions.

## Features

- Stats path resolution from user input, `${OCTOCODE_HOME}/stats.json`, or `~/.octocode/stats.json`.
- Deterministic dashboard generation with `scripts/build_dashboard.mjs`.
- Local HTML output using `assets/template.html`.
- Total measured tool calls.
- Raw, response, and saved-character estimates.
- Approximate token-savings view.
- GitHub cache-hit and rate-limit avoidance signals.
- Error summaries and empty-state handling when stats are missing.
- `--no-open`, `--output`, `--allow-empty`, and explicit `--stats` workflows.

## How it works

The skill follows this flow:

```text
RESOLVE STATS -> RUN BUILDER -> REPORT PATH + KEY NUMBERS
```

It resolves the stats file, calls the builder script, and reports the dashboard path, stats source, total calls, estimated savings, cache behavior, and errors. The skill does not recalculate metrics in chat; it delegates calculations to the script so dashboard and summary stay consistent.

## Internal flow

1. Check for a user-provided stats path.
2. Fall back to `${OCTOCODE_HOME}/stats.json`.
3. Fall back to `~/.octocode/stats.json`.
4. If no stats exist, explain that Octocode must run first or render an empty-state dashboard when requested.
5. Run `node skills/octocode-stats/scripts/build_dashboard.mjs` with the chosen flags.
6. Return the generated dashboard path and the key numbers.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-stats
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-stats
```

## Benefits

- Turns raw telemetry into a readable local report.
- Helps users see where Octocode saves context and repeated work.
- Makes cache, rate-limit, and error patterns visible.
- Keeps metric math deterministic and reproducible.

## For developers

Keep deterministic parsing, calculation, HTML rendering, and browser-opening behavior inside `scripts/build_dashboard.mjs`. Keep interpretation caveats in `references/measurement-notes.md`, transport/setup notes in `references/octocode.md`, and the visual shell in `assets/template.html`.
