# Output Format

Load when presenting results, building result cards, gating the next step, or running a deep-dive.

## Present results

Lead with the recommendation in one sentence. Group only when useful: `Best matches`, `Useful alternatives`, `Explore if...`. Few results → compact cards; many → list confirmed names/sources compactly and give detailed cards only for the strongest candidates. Keep prose short; never paste raw search dumps or large excerpts.

Card shape (label layout, not literal Markdown):

```text
Name:            <skill-name>  - fit: High | Medium | Low
Source:          <owner/repo path-to-SKILL.md> or <local path>
What it does:    <one sentence in your own words>
Actual flow:     <2-4 short steps from inspected content>
Quality signals: <specific evidence>
Why it matches:  <tie to user's request>
Caveat:          <real risk, or "None obvious from inspected files">
```

## Next-step gate

End with a gate offering the real branches — not just "install or cancel". Use a structured ask tool when the runtime provides one; otherwise present concise numbered choices and wait.

```text
Recommended: <skill-name> from <source>

Choose:
1. Install — fetch into agent destination(s) the user picks (see install-reference.md + fetch-and-create-locally.md).
2. Create a local skill — adapt patterns into a new local SKILL.md (see create-local-skill.md).
3. Explain — break down trigger, workflow, gates, and risks.
4. Show link — return the source URL or local path only, no write.
5. Compare — line up against another candidate.
6. Keep researching.
7. Cancel.
```

## Deep-dive

When the user picks a skill:

1. Fetch full `SKILL.md`.
2. Fetch directly referenced files that affect behavior.
3. Summarize trigger, workflow, support files, validation and safety gates, strengths, gaps, and adaptation ideas.
4. Ask whether to install, adapt into a local skill, compare, or keep researching.
