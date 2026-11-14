# localSearch questions

Use the [shared question protocol](README.md), [CLI/MCP checks](../../CHECKS.md#every-tool-every-time) and [expert alternatives](../../COMPARISONS.md). Assign `FIXTURE` before either arm starts; the 2026-11-14 round pins `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` with the adjacent `fixture-manifest.json`. Expected sets and source oracles stay in the campaign, outside these prompts.

## LS-01

Find every whole-word literal `beacon` occurrence in `FIXTURE/data/hits.txt`, returning source lines with identity. Distinguish occurrence count from matching-line count. Compare against a policy-matched `rg --no-config -n -w -F` query.

## LS-02

For `FIXTURE/data`, list files containing `beacon`, files without it, and matching-line counts sorted by count. Select ordinary `.txt` files, including empty files. Compare each view to the corresponding lean rg form; do not confuse a line count with individual matches.

## LS-03

Recover every selected hit and its context for whole-word `beacon` in `FIXTURE/data/hits.txt` using a small row limit and `responseChars:1200`. Reconstruct ordered source identities exactly once, including the Unicode text and final selected hit. Compare to the complete policy-matched rg answer.

## LS-04

Locate `beacon` in `FIXTURE/data/longline.txt`; get enough source around the hit to establish its column and content, then follow any required read to recover the selected complete line. Separate the short-window task from the full-line task when comparing output sizes.

## LS-05

Search `FIXTURE/data/hits.txt` for the absent literal `not_present_741`; assess whether the response states the scanned scope and gives a usable next step. Then submit the nearby invalid input `matchStrings` instead of `matchString` and assess whether the rejection identifies the actual field without running a search.

## LS-06

Repeat LS-01, then search a separate disposable copy after replacing one selected occurrence with an equally sized different word. Establish fresh results on both surfaces; retain original and edited file hashes and do not modify the frozen fixture.

## LS-07

In `FIXTURE/data/hits.txt`, select regex `beacon(?:less)?` with the schema-supported regex engine, first as content with one before/after context line and then as matchOnly with and without uniqueness. Compare ordered occurrences, deduplication policy and context union to corresponding rg commands. Then search `FIXTURE/data` with include `*.txt` and exclude `longline.txt`, keeping both arms on the same file set.
