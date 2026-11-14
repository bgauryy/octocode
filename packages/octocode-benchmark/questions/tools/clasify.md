# clasify questions

Use the [shared question protocol](README.md), [CLI/MCP checks](../../CHECKS.md#every-tool-every-time) and [expert alternatives](../../COMPARISONS.md). Assign `FIXTURE` before either arm starts; the 2026-11-14 round pins `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` with the adjacent `fixture-manifest.json`. Expected sets and source oracles stay in the campaign, outside these prompts.

## CL-01

Evaluate `FIXTURE/notes/restart.md` with three typed questions: whether changing the listening port requires restarting; whether changing only the theme requires restarting; which setting requires restarting (choice: theme, listening port, neither). Use harmless public content and retain deciding source lines separately. Record provider availability and every billed request.

## CL-02

Evaluate the note as a file source and as a localFetch query source, and compare answers to CL-01. Read each emitted source read before citing it. Compare main-agent context with reading the same note directly, including question inputs and usage receipts.

## CL-03

Evaluate per-item content with two independent harmless records: one states that changing a port requires restart, the other that a theme change needs no restart. Ask whether the action in each record requires restart. Verify independent item identity and unknown handling rather than merging records.

## CL-04

Run sections over the separate frozen `classification-sections` folder adjacent to `FIXTURE`, assigned by `classification-sections-manifest.json`, selecting a small response page. Ask a `type:"noul"` yes-probability question “Does changing the listening port require restarting the service?” independently of each section. Freeze the actual note list and hashes before execution. Follow all section/candidate and unfiltered continuations needed to measure recall and identify omissions.

## CL-05

Submit the nearby invalid typed question `type:"boolean"` instead of a schema-supported type, then a valid request in an isolated environment without provider configuration. Assess rejection versus missingConfiguration, provider request counts and MCP exposure honestly.

## CL-06

Repeat CL-01 as a separate request and report the billed-request increment, rather than claiming a cache hit from lower time. Only assess edit freshness on a disposable harmless copy. If the provider is unavailable, mark positive-path and efficiency dimensions unverified; do not infer a classification answer from an error.
