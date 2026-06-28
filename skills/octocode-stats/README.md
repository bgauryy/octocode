# Octocode Stats

`octocode-stats` builds a local HTML dashboard from Octocode MCP usage stats. It is for users who want to see response savings, cache behavior, errors, and rate-limit effects without manually reading `stats.json`.

The dashboard builder script owns the math and rendering.

## How it works

The skill resolves the stats source from an explicit path, `${OCTOCODE_HOME}/stats.json`, or `~/.octocode/stats.json`, then runs `scripts/build_dashboard.mjs` to generate a local HTML report. It summarizes measured calls, savings estimates, cache behavior, rate-limit signals, and tool errors, including an empty-state dashboard when stats are missing but the user still wants the view.

## Good asks

- "Show my Octocode stats."
- "Open the usage dashboard."
- "How many approximate tokens or characters did Octocode save?"
- "Check GitHub cache hits and avoided rate limits."
- "Render this specific stats file."
- "Review Octocode tool errors from the stats data."

## What you get

- A generated dashboard path.
- The stats source path.
- Total measured tool calls.
- Approximate tokens and characters saved.
- GitHub cache hits and rate-limit avoidance signals.
- Error counts and missing-stats notes when relevant.

## Where it reads from

The skill resolves stats from `${OCTOCODE_HOME}/stats.json`, then `~/.octocode/stats.json`, unless the user provides a specific stats path. It can render an empty-state dashboard when requested.

## Use another skill when

- The user is asking about credentials, MCP install, or configuration: use regular Octocode setup guidance.
- The user wants research about a codebase: route to the `octocode` or `octocode-engineer` skill.
- The user wants durable memory or handoff data: use `octocode-awareness`.

## User value

This skill turns a raw telemetry file into a readable local dashboard and a short summary, so users can understand Octocode's practical savings and failure patterns at a glance.
