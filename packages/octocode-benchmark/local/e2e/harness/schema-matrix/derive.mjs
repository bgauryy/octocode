// The published query schema owns the matrix: every branch, every finite value,
// whether a row is admitted. No value is named here, so a new one enters the plan
// by itself and, with nothing to judge it, fails acceptance under its own name.
import { createAjv } from '../query-defaults.mjs';
import { oneFactor, pairwise } from './pairwise.mjs';

const pointer = (root, ref) => ref?.startsWith('#/') ? ref.slice(2).split('/').reduce((node, key) => node?.[key.replaceAll('~1', '/').replaceAll('~0', '~')], root) : undefined;
const follow = (root, schema) => { let node = schema; while (node?.$ref) node = pointer(root, node.$ref); return node; };
const finite = schema => schema.enum ?? (Object.hasOwn(schema, 'const') ? [schema.const] : schema.type === 'boolean' ? [false, true] : []);
export const keyOfPath = path => path.join('.');

/** The object forms a query may take: nested unions flatten, each form keeps the shared `$defs`. */
export function branchesOf(root) {
  const out = [];
  const visit = (schema, inherited) => {
    const resolved = follow(root, schema);
    const checks = [...inherited, ...(resolved['x-checks'] ?? [])];
    const union = resolved.anyOf ?? resolved.oneOf;
    if (union) { for (const member of union) visit(member, checks); return; }
    out.push({ schema: { ...resolved, $defs: root.$defs }, checks });
  };
  visit(root, []);
  return out;
}

function noteAxis(found, path, values, { required, sibling }) {
  const key = keyOfPath(path);
  const axis = found.get(key) ?? { key, path, values: [], required: false, sibling: false };
  for (const value of values) if (!axis.values.includes(value)) axis.values.push(value);
  axis.required ||= required;
  axis.sibling ||= sibling;
  found.set(key, axis);
}

/** Finite fields of one form: `required` always, `sibling` whenever their parent is present. */
export function axesOf(branch) {
  const found = new Map();
  const visit = (schema, path, flags, ancestors) => {
    const resolved = follow(branch.schema, schema);
    if (!resolved || typeof resolved !== 'object' || ancestors.has(resolved)) return;
    const seen = new Set([...ancestors, resolved]);
    const values = finite(resolved);
    if (values.length) noteAxis(found, path, values, flags);
    for (const member of ['allOf', 'anyOf', 'oneOf'].flatMap(name => resolved[name] ?? [])) visit(member, path, flags, seen);
    for (const [name, child] of Object.entries(resolved.properties ?? {})) {
      const named = (resolved.required ?? []).includes(name);
      visit(child, [...path, name], { required: named && flags.present, sibling: named, present: named && flags.present }, seen);
    }
    const inner = { required: false, sibling: false, present: false };
    if (resolved.items) visit(resolved.items, [...path, '[]'], inner, seen);
    if (typeof resolved.additionalProperties === 'object') visit(resolved.additionalProperties, [...path, '*'], inner, seen);
  };
  visit(branch.schema, [], { required: false, sibling: false, present: true }, new Set());
  return [...found.values()];
}

/** Write a value at a schema path: `[]` holds one item, `*` one named entry. */
export function setPath(target, path, value) {
  let node = target;
  path.forEach((part, index) => {
    const last = index === path.length - 1;
    const next = path[index + 1];
    const key = part === '[]' ? 0 : part === '*' ? 'k' : part;
    if (last) { node[key] = value; return; }
    node[key] ??= next === '[]' ? [] : {};
    node = node[key];
  });
  return target;
}

/** Sibling fields a parent requires together (`sort.by` and `sort.order`) are one axis of joined values. */
function joinRequired(axes) {
  const parents = new Map();
  for (const axis of axes) {
    const parent = axis.path.length > 1 && axis.sibling ? keyOfPath(axis.path.slice(0, -1)) : null;
    if (parent) (parents.get(parent) ?? parents.set(parent, []).get(parent)).push(axis);
  }
  const joined = new Set([...parents.values()].filter(group => group.length > 1).flat());
  const combined = [...parents].filter(([, group]) => group.length > 1).map(([key, group]) => ({
    key, parts: group, path: group[0].path.slice(0, -1), required: false,
    values: group.reduce((rows, axis) => rows.flatMap(row => axis.values.map(value => ({ ...row, [axis.path.at(-1)]: value }))), [{}]),
  }));
  return [...axes.filter(axis => !joined.has(axis)), ...combined];
}

const assignmentRow = (axes, assignment) => {
  const row = {};
  for (const axis of axes) if (Object.hasOwn(assignment, axis.key) && axis.path) setPath(row, axis.path, assignment[axis.key]);
  return row;
};

const shared = createAjv();
const compiled = new WeakMap();
const compile = schema => compiled.get(schema) ?? compiled.set(schema, shared.compile(schema)).get(schema);

/** Cross-field checks the schema carries beside its shape. */
function checksPass(branch, row) {
  for (const check of branch.checks) {
    const items = check.each ? Object.values(row[check.each] ?? {}) : [row];
    for (const item of items) if ((!check.when || compile(check.when)(item)) && !compile(check.assert)(row)) return false;
  }
  return true;
}

/**
 * Every row one tool's schema admits, from `spec.pseudo` axes (fixture choices the
 * schema does not know) and `spec.base(assignment, { fixed })`, which adds the
 * required free-form fields. Rows: one per value alone, then all pairs.
 */
export function planTool(tool, spec) {
  const root = tool.querySchema;
  const cases = [];
  const seen = new Set();
  for (const [index, branch] of branchesOf(root).entries()) {
    const axes = joinRequired(axesOf(branch));
    const fixed = axes.filter(axis => axis.required && axis.values.length === 1);
    const free = axes.filter(axis => !fixed.includes(axis));
    const pseudo = spec.pseudo ?? [];
    const pinned = Object.fromEntries(fixed.map(axis => [axis.key, axis.values[0]]));
    const rowOf = assignment => {
      const full = { ...pinned, ...assignment };
      const row = spec.base({ ...full, ...assignmentRow(axes, full) }, full);
      if (!row) return null;
      const merged = { ...row, ...mergeDeep(assignmentRow(axes, full), row) };
      return spec.shape?.(merged) ?? merged;
    };
    const validate = compile(branch.schema);
    const required = free.filter(axis => axis.required);
    const withRequired = assignment => {
      const out = { ...assignment };
      for (const axis of required) if (!Object.hasOwn(out, axis.key)) out[axis.key] = axis.values[0];
      return out;
    };
    const valid = assignment => {
      const row = rowOf(withRequired(assignment));
      return !!row && validate(row) && checksPass(branch, row);
    };
    const add = (assignment, kind) => {
      const full = withRequired(assignment);
      const query = rowOf(full);
      const identity = JSON.stringify([query, Object.fromEntries(pseudo.map(axis => [axis.key, full[axis.key]]))]);
      if (seen.has(identity)) return;
      seen.add(identity);
      const all = { ...pinned, ...full };
      cases.push({
        id: `${tool.name}#${cases.length}`, tool: tool.name, branch: index, kind, assignment: all, query, values: valuesOf(axes, all),
        // The same scene with one field changed or unset, to prove a field changes the answer.
        swaps: free.flatMap(axis => [...axis.values.filter(value => value !== full[axis.key]), undefined].flatMap(value => {
          const changed = { ...full };
          if (value === undefined) delete changed[axis.key]; else changed[axis.key] = value;
          if (value === undefined && !Object.hasOwn(full, axis.key)) return [];
          return valid(changed) ? [{ field: axis.key, assignment: { ...pinned, ...withRequired(changed) }, query: rowOf(withRequired(changed)) }] : [];
        })),
      });
    };
    if (valid({})) add({}, 'single');
    for (const assignment of oneFactor(free, valid)) add(assignment, 'single');
    for (const assignment of pairwise([...free, ...pseudo], valid)) add(assignment, 'pair');
  }
  return cases;
}

function mergeDeep(a, b) {
  if (!a || typeof a !== 'object' || Array.isArray(a) || !b || typeof b !== 'object' || Array.isArray(b)) return a;
  const out = { ...b };
  for (const [key, value] of Object.entries(a)) out[key] = Object.hasOwn(b, key) ? mergeDeep(value, b[key]) : value;
  return out;
}

/** The (field, value) facts a row claims, for the coverage ledger. */
function valuesOf(axes, assignment) {
  return axes.filter(axis => Object.hasOwn(assignment, axis.key)).flatMap(axis => axis.parts
    ? axis.parts.map(part => [part.key, assignment[axis.key][part.path.at(-1)]])
    : [[axis.key, assignment[axis.key]]]);
}

/** A value the schema declares that no planned row carries, by tool, path and value. */
export function unplanned(tool, cases) {
  const planned = new Set(cases.flatMap(entry => entry.values.map(pair => JSON.stringify(pair))));
  return branchesOf(tool.querySchema).flatMap(branch => axesOf(branch)).flatMap(axis => axis.values
    .filter(value => !planned.has(JSON.stringify([axis.key, value])))
    .map(value => ({ tool: tool.name, field: axis.key, value })));
}
