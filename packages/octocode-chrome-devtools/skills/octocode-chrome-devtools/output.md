# Output

Lead with the supported finding, source URL or capture path, observed post-action state and gaps. Keep screenshots beside the claim, with alt text and viewport context. Link full JSON, HAR and traces; never paste them.

## Saved result

Captures keep their returned formats under `<workspace>/.octocode/octocode-chrome-devtools/tmp/`; durable `session` captures use `<workspace>/.octocode/chromedevtools/<id>/`. Reports use `<workspace>/.octocode/octocode-chrome-devtools/<YYYY-MM-DD>-<slug>.md`. Workspace is the repository root, else cwd, never an existing `.octocode/`. A user-named destination wins; subagent scratch does not.

## Saved report

Save when requested or when captures and steps need a reusable report; otherwise answer in chat.

Start with YAML frontmatter `status` (`Pass`, `Fail` or `Partial`) and `question`, then a `#` title and a lead with the finding. Then `## Finding`, `## Steps` and `## Gaps` (`None.` when none). Steps is one table:

| # | Action | Observed state | Capture | Result |
|---|---|---|---|---|

Result: `Pass`, `Fail`, or `Partial`. Capture files (screenshots, HAR, traces): add `captures: <workspace-relative capture folder>` to the frontmatter and a `## Captures` section linking each; inline-only captures get `inline only` in the Capture cell.

For unfinished work, set `next` in the frontmatter and retain state, evidence, attempts, blockers, constraints and required target, frame and session IDs. When someone else's action is needed, lead with situation, background, assessment and recommendation (SBAR).
