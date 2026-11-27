# localFetch questions

Use the [shared question protocol](README.md), [CLI/MCP checks](../../CHECKS.md#every-tool-every-time) and [expert alternatives](../../COMPARISONS.md). Assign `FIXTURE` before either arm starts; the 2026-11-14 round pins `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` with the adjacent `fixture-manifest.json`. Expected sets and source oracles stay in the campaign, outside these prompts.

## LF-01

Read inclusive lines 4–9 from `FIXTURE/src/calls.ts`. Preserve exact line text and source identity; compare with `sed -n` on the same frozen file.

## LF-02

Read the entire declaration named `longUnicode` from `FIXTURE/src/long.ts`, without guessing its ending line. Use a small response size, follow all required pages, and prove that the selected declaration ends before the next declaration. Compare against ast-grep-derived declaration range followed by sed, counting both rival steps.

## LF-03

Read the full `FIXTURE/data/hits.txt` with small line and response windows. Reconstruct all selected lines exactly once, preserving Unicode. Also compare symbols and compact views of `FIXTURE/src/calls.ts` to the explicitly selected representation rather than full source.

## LF-04

Locate `beacon` inside `FIXTURE/data/longline.txt`, then obtain the complete selected line via its emitted continuations. Check source identity and byte/code-point offsets. Compare a short window to a short window and a complete line to a complete line.

## LF-05

Read `FIXTURE/data/empty.txt` and request a missing path under `FIXTURE/data`. Submit the nearby invalid field `range` instead of `ranges`. Assess empty, not-found and input-error identity, repair guidance, and CLI/MCP parity.

## LF-06

Repeat LF-01, then read a disposable copy before and after editing a selected line. Record hashes and establish whether the next read reflects the source; do not alter the frozen fixture.

## LF-07

Read overlapping ranges 1–3 and 3–5 from `FIXTURE/src/calls.ts`. Establish the documented overlap/ordering behavior and preserve source text under small response windows; compare an expert range union or concatenation matching that selection. Request an absent matchString in the same file and distinguish no match from an empty file.
