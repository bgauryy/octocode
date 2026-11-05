// All-pairs row generation with constraints. A full product of a tool's finite
// inputs is astronomically large and adds little; every pair of values still
// meets in at least one row, so an interaction between any two fields is run.
// Axes may stay unset: every field here is optional unless the schema says so.
const pairId = (a, va, b, vb) => JSON.stringify([a, va, b, vb]);

/** Every admitted pair of values across two axes, unordered by axis position. */
function admittedPairs(axes, valid) {
  const pairs = new Map();
  const add = (a, b) => {
    for (const va of a.values) for (const vb of b.values) {
      if (valid({ [a.key]: va, [b.key]: vb })) pairs.set(pairId(a.key, va, b.key, vb), [a.key, va, b.key, vb]);
    }
  };
  for (const [i, a] of axes.entries()) for (const b of axes.slice(i + 1)) add(a, b);
  return pairs;
}

/** `axes`: [{ key, values, required? }]; `valid(assignment)`: can this partial row exist. */
export function pairwise(axes, valid) {
  const uncovered = admittedPairs(axes, valid);
  const position = new Map(axes.map((axis, index) => [axis.key, index]));
  const idOf = (x, vx, y, vy) => (position.get(x) < position.get(y) ? pairId(x, vx, y, vy) : pairId(y, vy, x, vx));
  const gain = (row, key, value) => Object.entries(row).filter(([other, otherValue]) => uncovered.has(idOf(key, value, other, otherValue))).length;
  const extend = (row, axis) => {
    let best, bestGain = axis.required ? -1 : 0;
    for (const value of axis.values) {
      const gained = gain(row, axis.key, value);
      if (gained > bestGain && valid({ ...row, [axis.key]: value })) { best = value; bestGain = gained; }
    }
    if (best !== undefined) row[axis.key] = best;
  };
  const rows = [];
  while (uncovered.size) {
    const [a, va, b, vb] = uncovered.values().next().value;
    const row = { [a]: va, [b]: vb };
    for (const axis of axes) if (!Object.hasOwn(row, axis.key)) extend(row, axis);
    const entries = Object.entries(row);
    for (const [index, [x, vx]] of entries.entries()) for (const [y, vy] of entries.slice(index + 1)) uncovered.delete(idOf(x, vx, y, vy));
    rows.push(row);
  }
  return rows;
}

/** Every value alone: the row where its own effect stands out from the base. */
export function oneFactor(axes, valid) {
  return axes.flatMap(axis => axis.values.map(value => ({ [axis.key]: value })).filter(valid));
}
