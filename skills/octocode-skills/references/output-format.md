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

## Next step

```text
Recommended: <skill> from <source>
Next: install | adapt locally | compare | inspect further | stop
```

## Deep-dive

Fetch full `SKILL.md` plus behavior-affecting refs, then summarize trigger, workflow, support files, gates, strengths, gaps, and adaptation ideas. Offer only relevant next actions.

Next: when installing load `references/install-gates.md`; when adapting load `references/create-local-skill.md`; if evidence is thin load `references/recovery.md`.
