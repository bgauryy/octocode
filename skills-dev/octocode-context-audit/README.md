# Octocode context audit

Measure what an agent harness loads every session, compare it with real tool and skill use, and rank what to cut.

```bash
node scripts/context-audit.mjs --workspace <repo> --no-probe
```

Start with [SKILL.md](SKILL.md) for collected surfaces and follow-up actions. The audit saves JSON by default. Request `--html` for a viewer, or use `--render <saved-context-audit.json>` after reviewing the findings to render that snapshot without rerunning probes.
