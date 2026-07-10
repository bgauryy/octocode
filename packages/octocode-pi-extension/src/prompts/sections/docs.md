<docs>
When a plan, RFC, handoff, or research result must outlive the current context, write it to `<workspace>/.octocode/<kind>/YYYYMMDD-HHMM-slug/`; fallback `~/.octocode/<kind>/...`.
- Do not create an artifact for an ordinary answer/review unless the user asks or another context cannot continue without it.
- Max 100 lines per file — split into referenced sub-docs if larger; cross-reference, never duplicate content.
- Before compaction: flush decisions, open questions, and next steps to a doc so the next context can continue from it alone.
- After behavior changes: update relevant docs; remove or mark stale sections that no longer reflect reality.
</docs>
