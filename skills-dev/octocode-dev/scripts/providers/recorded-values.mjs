// What the recorded-answer checks count as a value: the keys that name fields, the
// shortest text that says anything, the longest text kept whole, and the values a
// record holds. Shared by the shape checks (recorded-shape.mjs) and request routing
// (recorded-routing.mjs).

export const FIELD_KEY = /^[A-Za-z_@$][\w@$:-]*$/;
/** Text past this many characters may be cut to its start. */
export const LONG_TEXT = 1200;
/** A value this short says nothing when it appears in an output. */
export const MIN_EVIDENCE = 3;

/** Every text and number in `value`, and every key that names data, at any depth. */
export function leaves(value, found = new Set()) {
  if (typeof value === 'string' || typeof value === 'number') found.add(String(value));
  else if (value && typeof value === 'object') for (const [key, child] of Object.entries(value)) {
    if (!Array.isArray(value) && !FIELD_KEY.test(key)) found.add(key);
    leaves(child, found);
  }
  return found;
}
