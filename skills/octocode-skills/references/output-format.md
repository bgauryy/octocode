# Output Format

Load when presenting results, gating next steps, or deep-diving a candidate. Why: consistent cards + real branches, not raw dumps.

## Present

Lead with the recommendation in one sentence. Group only when useful: Best matches / Useful alternatives / Explore if….

Few results → compact cards. Many → list names/sources; detail only the strongest. Never paste raw search dumps.

```text
Name:            <skill> — fit: High | Medium | Low
Source:          <owner/repo/path> or <local path>
What it does:    <one sentence>
Actual flow:     <2-4 steps from inspected content>
Quality signals: <specific evidence>
Why it matches:  <tie to request>
Caveat:          <real risk, or "None obvious">
```

## Next-step gate

```text
Recommended: <skill> from <source>
1. Install — destinations via install-gates.md + fetch-remote.md
2. Create local — adapt via create-local-skill.md
3. Explain — trigger, workflow, gates, risks
4. Show link — URL/path only, no write
5. Compare — vs another candidate
6. Keep researching
7. Cancel
```

## Deep-dive

Fetch full `SKILL.md` + behavior-affecting refs → summarize trigger, workflow, support files, gates, strengths, gaps, adaptation ideas → ask install / adapt / compare / research.

Next: when installing load `references/install-gates.md`; when adapting load `references/create-local-skill.md`; if evidence is thin load `references/recovery.md`.
