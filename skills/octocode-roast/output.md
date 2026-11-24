# Roast output

<!-- report-contract {"statuses":["Blocking","Needs work","Acceptable"],"required":["Findings","Fix order"],"anyOf":[]} -->

Lead with the worst proven finding and a scope-qualified assessment. Use **Blocking** only for a demonstrated blocker; when a PR merge verdict is requested, let `octocode-pr-review` own that decision. If none holds, name the reviewed scope. Write each ranked finding as the [finding row](SKILL.md#rank-write-deliver), and distinguish observed execution from source-supported inference. For example:

> **Critical · src/auth/middleware.ts:42.** An authenticated request → the request logger writes the bearer token → anyone with log access can replay the session. Evidence: traced from the route handler to `logger.info(req.headers)`. Repair: log the user ID and drop the header. Check: a test that captures logs finds the token now and none after. Confidence: high. Your log aggregator is now an identity provider.

Redact secrets. Recommend rotation for a real credential and claim exposure only with evidence. Explain where a substantial top failure enters, what it affects and how to repair it safely. Group lesser findings only if each retains its evidence and repair. Finish with the fix order as finding locations in sequence; repairs and checks stay with their findings.

- **Critique only:** Finish with the fix order.
- **Fixes requested:** Within the roasted scope, re-read current code, apply the smallest repair per finding (including removal of a real credential) and run relevant checks. Report repaired, failed and outstanding items.

## Saved report

Save when requested or the roast outgrows chat. Use `<workspace>/.octocode/octocode-roast/<YYYY-MM-DD>-<slug>.md`, with scratch in `<workspace>/.octocode/tmp/octocode-roast/`. Front matter takes `status` (verdict), `question`, optional `confidence` and `next` for pending work. Use `## Findings` and `## Fix order`. Accepted `status` values: `Blocking`, `Needs work`, `Acceptable`.

After each save, run `node <skill-folder>/scripts/render-report.mjs <file.md> --check` ([renderer](scripts/render-report.mjs)): exit 0 means the status and the `Findings`, `Fix order` sections match this contract; exit 2 lists each violation to fix before delivering. Render HTML only when requested, per the [shared report guidance](references/report-output.md). Keep Markdown, distinguish generation from browser inspection and disclose unverified rendering.
