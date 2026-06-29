# Octocode Stats

`octocode-stats` gives an agent a way to explain Octocode usage. It turns raw local telemetry into a readable picture of tool calls, response savings, cache behavior, avoided rate limits, and errors.

Use it when the user wants to understand how Octocode is performing over time rather than inspect code.

## The Problem

Telemetry is useful only when it becomes legible. A raw stats file can show counts and sizes, but users usually want the operational story: how much work was saved, where the cache helped, whether errors are clustered, and whether rate limits were avoided.

This skill gives the agent a dashboard-oriented interpretation mode for Octocode's own usage data.

## Capabilities

- Stats discovery from the active Octocode home or an explicit stats file.
- Local HTML dashboard generation for visual inspection.
- Total measured tool-call reporting.
- Raw, response, and saved-character estimates.
- Approximate token-savings view.
- Cache-hit and rate-limit avoidance signals.
- Error summaries and empty-state behavior.
- Consistent metric interpretation so the dashboard and chat summary agree.

## Operating Model

The workflow is:

```text
RESOLVE STATS -> BUILD DASHBOARD -> REPORT PATH + KEY NUMBERS
```

The agent locates the relevant stats source, creates a local dashboard, and reports the key numbers in plain language. It does not turn telemetry into product claims; it presents the measured data with the caveats users need.

## User Experience

Users get a local report they can open, plus a short explanation of the most important numbers. The skill is useful for understanding savings, cache effectiveness, error patterns, and whether Octocode has enough activity to evaluate.

It is not for credential setup, MCP installation, or code research. Those belong to Octocode setup or research skills.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-stats
```

## Maintainer Notes

Keep this README focused on telemetry interpretation. Keep metric caveats, transport notes, and dashboard implementation details in the agent-facing skill file and references so users see the story before the plumbing.
