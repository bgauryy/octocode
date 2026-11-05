// What the run must prove: every finite value judged on each surface, every field's
// values distinguishable on the fixture, or a stated reason why not.
import { finiteInputDomains } from '../tool-health/input-coverage.mjs';
import { firstDifference } from './runner.mjs';

const fieldOf = domain => domain.path.join('.');

export function coverageReport({ tools, proved, specs, surfaces }) {
  const uncovered = [];
  const acceptedOnly = [];
  const summary = { total: 0, ...Object.fromEntries(surfaces.map(surface => [surface, { proved: 0, missing: 0 }])) };
  for (const tool of tools) for (const domain of finiteInputDomains(tool.querySchema)) for (const value of domain.values) {
    summary.total += 1;
    const field = fieldOf(domain);
    const seen = proved.get(`${tool.name}\0${field}\0${JSON.stringify(value)}`) ?? new Set();
    const absent = surfaces.filter(surface => !seen.has(surface));
    for (const surface of surfaces) summary[surface][absent.includes(surface) ? 'missing' : 'proved'] += 1;
    if (!absent.length) continue;
    const reason = specs[tool.name]?.acceptedOnly?.[field];
    if (reason) acceptedOnly.push({ tool: tool.name, field, value, reason });
    else uncovered.push({ tool: tool.name, field, value, surfaces: absent });
  }
  return { summary, uncovered, acceptedOnly };
}

/**
 * A field whose values all yield the same truth on this fixture proves nothing: a
 * tool ignoring the field would still pass. Each row is rejudged with one field
 * changed or unset; a field is proved useful once any such change moves the truth.
 */
export function discriminationProblems(plan, specs, context) {
  const moved = new Set();
  const fields = new Map();
  for (const row of plan) {
    const spec = specs[row.tool];
    if (!spec.truth) continue;
    const base = spec.truth(context, row);
    if (!base) continue;
    for (const swap of row.swaps) {
      const key = `${row.tool}\0${swap.field}`;
      fields.set(key, { tool: row.tool, field: swap.field });
      if (moved.has(key) || spec.indistinct?.[swap.field]) continue;
      const swapped = spec.truth(context, { ...row, assignment: { ...swap.assignment, ...pseudoOf(row, swap) }, query: swap.query });
      if (swapped && firstDifference(base, swapped)) moved.add(key);
    }
  }
  return [...fields].filter(([key, entry]) => !moved.has(key) && !specs[entry.tool].indistinct?.[entry.field]).map(([, entry]) => entry);
}

const pseudoOf = (row, swap) => Object.fromEntries(Object.entries(row.assignment).filter(([name]) => !Object.hasOwn(swap.assignment, name) && name !== swap.field));
