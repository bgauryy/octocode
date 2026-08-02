/**
 * Re-anchor GitHub text-match positions after the fragment has been
 * transformed (sanitized/minified). GitHub's indices point into the RAW
 * fragment; the fragment we return to the agent is not that string, so raw
 * indices would point at the wrong text. Each position is re-located by
 * searching for its matched TEXT in the transformed fragment (forward scan
 * keeps repeated occurrences distinct and ordered). A match the transform
 * removed (e.g. a comment the minifier stripped) is dropped rather than
 * emitted with a stale offset — a missing anchor is honest, a wrong one lies.
 */
export function recomputeMatchPositions(
  rawFragment: string,
  rawPositions: ReadonlyArray<readonly [number, number]>,
  transformedFragment: string
): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  let searchFrom = 0;
  for (const [start, end] of rawPositions) {
    const matchedText = rawFragment.slice(start, end);
    if (!matchedText) continue;
    let idx = transformedFragment.indexOf(matchedText, searchFrom);
    if (idx === -1) {
      // Occurrences can collapse or reorder under minification — retry from
      // the top before concluding the match text is gone entirely.
      idx = transformedFragment.indexOf(matchedText);
      if (idx === -1) continue;
    }
    out.push([idx, idx + matchedText.length]);
    searchFrom = idx + matchedText.length;
  }
  return out;
}
