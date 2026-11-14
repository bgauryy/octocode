# structureSearch questions

Use the [shared question protocol](README.md), [CLI/MCP checks](../../CHECKS.md#every-tool-every-time) and [expert alternatives](../../COMPARISONS.md). Assign `FIXTURE` before either arm starts; the 2026-11-14 round pins `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` with the adjacent `fixture-manifest.json`. Expected sets and source oracles stay in the campaign, outside these prompts.

## ST-01

List the directory tree rooted at `FIXTURE/tree` at depth 2, selecting names and directory identity. Compare with policy-matched find or fd and establish which depth convention each uses.

## ST-02

List all ordinary files under `FIXTURE/tree`, ordered by descending byte size, then by descending line count. Return the metric requested for every row and compare with stat and wc; keep ties explicit.

## ST-03

List all ordinary files under `FIXTURE` with row limit 2 and `responseChars:1200`; walk every required page. Reconstruct the selected path set exactly once. Repeat in details view and verify the same membership.

## ST-04

Apply a filename regex for `parent` and a separate one for `child` under `FIXTURE/tree`. Establish whether the filter means a basename or path component from the schema and output, and compare the same selection to an expert command.

## ST-05

Request a filename that is absent under `FIXTURE/tree`, then submit `operation:"files"` with the nearby invalid sort field `size` in place of the schema-defined size metric. Assess scope disclosure and actionable rejection on CLI and MCP.

## ST-06

Repeat ST-02, then add one file in a disposable tree copy. Verify membership and ordering refresh without changing the frozen fixture. State why fixed equal mtimes make the fixture unsuitable for a unique newest-file winner; create a recorded disposable timestamp variation before testing modified-time order.

## ST-07

Find empty `.txt` files under `FIXTURE/data`, and separately list only directories under `FIXTURE/tree`. Compare extension and entry-type semantics with a policy-matched expert find command. Ensure a zero-byte file remains a real selected row.
