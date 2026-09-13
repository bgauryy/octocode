/** Authoring guidance for generated instructions and tool-routing descriptions. */
export const BEHAVIORAL_PROMPT_GUIDANCE =
  'Define a concrete action and the condition that selects it. Add the smallest useful wrong/right example or consequence when it resolves ambiguity; do not force every instruction into a fixed sequence. '
  + 'For example, "be efficient" gives no decision boundary; "reuse an observed schema; describe only an unfamiliar tool" changes the next call. '
  + 'Keep only sentences that distinguish behavior, establish a boundary, explain a consequence, or direct action. Prefer examples to redundant explanation. '
  + 'Use literal language without role-play, motivational prose, repeated rules, or empty headings. Put exact field constraints in schemas and selection guidance in descriptions. '
  + 'Preserve identifiers, required metadata, authority, and output format. Optimize behavioral information per token, not minimum length.';
