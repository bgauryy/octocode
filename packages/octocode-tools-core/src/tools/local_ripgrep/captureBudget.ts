// Capture budget for structural (AST) search results — keeps `$$$` list
// captures from turning a handful of matches into tens of KB of duplicated
// source. Extracted from structuralSearch.ts.

export type MetavarRange = {
  text: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
};

const CAPTURE_TEXT_BUDGET = 120;
const COMMENT_PREFIX = /^\s*(\/\/|\/\*|\*|#)/;

/**
 * Apply the default capture budget to one match's metavar payload.
 *
 * `$$$` list captures (bodies, argument lists) serialize every statement
 * TWICE (metavars text + metavarRanges text) — for a handful of function
 * matches that is tens of KB of duplicated source an agent almost never
 * needs: the ranges are the useful part (LSP anchors). Default policy:
 *  - `metavars`: keep single-entry captures (cheap, high-value anchors like
 *    `$NAME`), drop multi-entry list captures entirely.
 *  - `metavarRanges`: always keep (they are the anchors), but prune
 *    comment-only entries from list captures and truncate each entry's text
 *    to CAPTURE_TEXT_BUDGET chars (marked with `…`).
 * `captureText:true` restores the verbatim engine payload.
 */
export function budgetCaptures(
  metavars: Record<string, string[]> | undefined | null,
  metavarRanges: Record<string, MetavarRange[]> | undefined | null,
  captureText: boolean
): {
  metavars?: Record<string, string[]>;
  metavarRanges?: Record<string, MetavarRange[]>;
  capturesTruncated?: true;
} {
  const hasRanges = metavarRanges && Object.keys(metavarRanges).length > 0;
  if (captureText) {
    return {
      ...(metavars ? { metavars } : {}),
      ...(hasRanges ? { metavarRanges: metavarRanges ?? undefined } : {}),
    };
  }

  let capturesTruncated = false;
  let budgetedMetavars: Record<string, string[]> | undefined;
  if (metavars) {
    budgetedMetavars = {};
    for (const [name, values] of Object.entries(metavars)) {
      if (values.length <= 1) {
        if (values.some(value => value.length > CAPTURE_TEXT_BUDGET)) {
          capturesTruncated = true;
        }
        budgetedMetavars[name] = values.map(truncateCapture);
      } else {
        capturesTruncated = true;
      }
      // Multi-entry list capture: text omitted — anchors live in ranges.
    }
    if (Object.keys(budgetedMetavars).length === 0)
      budgetedMetavars = undefined;
  }

  let budgetedRanges: Record<string, MetavarRange[]> | undefined;
  if (hasRanges && metavarRanges) {
    budgetedRanges = {};
    for (const [name, ranges] of Object.entries(metavarRanges)) {
      const isListCapture = ranges.length > 1;
      const selected = isListCapture
        ? ranges.filter(r => !COMMENT_PREFIX.test(r.text))
        : ranges;
      if (
        selected.length !== ranges.length ||
        selected.some(range => range.text.length > CAPTURE_TEXT_BUDGET)
      ) {
        capturesTruncated = true;
      }
      const kept = selected.map(r => ({
        ...r,
        text: truncateCapture(r.text),
      }));
      if (kept.length > 0) budgetedRanges[name] = kept;
    }
    if (Object.keys(budgetedRanges).length === 0) budgetedRanges = undefined;
  }

  return {
    ...(budgetedMetavars ? { metavars: budgetedMetavars } : {}),
    ...(budgetedRanges ? { metavarRanges: budgetedRanges } : {}),
    ...(capturesTruncated ? { capturesTruncated: true as const } : {}),
  };
}

function truncateCapture(text: string): string {
  return text.length > CAPTURE_TEXT_BUDGET
    ? `${text.slice(0, CAPTURE_TEXT_BUDGET)}…`
    : text;
}
