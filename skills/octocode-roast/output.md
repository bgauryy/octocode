# Roast output

<!-- report-contract {"statuses":["Blocking","Needs work","Acceptable"],"required":["Findings","Fix order"],"anyOf":[]} -->

Lead with the worst proven finding and a scope-qualified assessment. Use **Blocking** only for a demonstrated blocker; when a PR merge verdict is requested, let `octocode-pr-review` own that decision. If none holds, name the reviewed scope. Give each ranked finding its `path:line` (or design section), trigger, mechanism, consequence, evidence and repair. Distinguish observed execution from source-supported inference. Pair the repair with a check that would fail before the fix and pass afterward. For example:

> **Critical · src/auth/middleware.ts:42.** The authenticated request path reaches a log call that writes the bearer token. Anyone with log access can replay the session. Log the user ID and drop the header. Confidence: high. Your log aggregator is now an identity provider.

Redact secrets. Recommend rotation for a real credential and claim exposure only with evidence. Explain where a substantial top failure enters, what it affects and how to repair it safely. Group lesser findings only if each retains its evidence and repair. Finish with fixes in order, each beside its proving check.

- **Critique only:** Finish with the fix order.
- **Fixes requested:** Within the roasted scope, re-read current code, apply the smallest repair per finding (including removal of a real credential) and run relevant checks. Report repaired, failed and outstanding items.

## Saved report

Save when requested or the roast outgrows chat. Use `<workspace>/.octocode/octocode-roast/<YYYY-MM-DD>-<slug>.md` and its `tmp/` for scratch. Front matter takes `status` (verdict), `question`, optional `confidence` and `next` for pending work. Use `## Findings` and `## Fix order`. Accepted `status` values: `Blocking`, `Needs work`, `Acceptable`.

Render HTML when requested and a report renderer is available (another installed skill may ship one). Keep Markdown, distinguish generation from browser inspection and disclose unverified rendering.
