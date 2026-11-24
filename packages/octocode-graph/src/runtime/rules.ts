// Dependency rules over a saved graph, in dependency-cruiser's vocabulary (rules-reference.md): `forbidden` rules (a
// direct dependency, a target `reachable` through any chain or not reachable at all, a `circular` dependency with `via`
// and `viaOnly`), `allowed` rules (any other dependency is `not-in-allowed`) and `required` rules (a module must
// depend on a target). `from.path` captures groups that `to.path` and `to.pathNot` take as $1…$9;
// `dependencyTypes` match the graph's import kinds. Each rule walks the imports its `load` keeps (runtime by default:
// no import type), over the same index and walks as the queries. Paths are regular expressions over node ids;
// one that matches nothing in the graph is an error, as `reach --forbid` refuses a misspelled target. scheme/rules.json
// is the one definition of the file: the loader validates against it.
import schema from '../scheme/rules.json' with { type: 'json' };
import { indexOf, type QueryData, type QueryIndex } from './query-index.ts';
import { walk } from './query-ops.ts';
import { chainHops, type Hop } from './query-reach.ts';
import { nearest } from './query-forgiving.ts';
import { keeps, loadOf, type LoadMode } from '../loads.ts';

export type Severity = 'error' | 'warn' | 'info';
export type RuleLoad = LoadMode | 'all';
type Pattern = string | string[];
interface Modules {
  path?: Pattern;
  pathNot?: Pattern;
}
interface ForbiddenRule {
  name: string;
  severity?: Severity;
  comment?: string;
  load?: RuleLoad;
  from: Modules;
  to: Modules & {
    reachable?: boolean;
    circular?: boolean;
    via?: Modules;
    viaOnly?: Modules;
    dependencyTypes?: string[];
    dependencyTypesNot?: string[];
  };
}
interface AllowedRule {
  comment?: string;
  load?: RuleLoad;
  from: Modules;
  to: Modules & { dependencyTypes?: string[]; dependencyTypesNot?: string[] };
}
interface RequiredRule {
  name: string;
  severity?: Severity;
  comment?: string;
  load?: RuleLoad;
  module: Modules;
  to: { path: Pattern; reachable?: boolean };
}
/** A rules file as written (scheme/rules.json). */
export interface RulesFile {
  forbidden?: ForbiddenRule[];
  allowed?: AllowedRule[];
  allowedSeverity?: Severity;
  required?: RequiredRule[];
}

/** One broken rule: the dependency (or module) that breaks it and the chain of imports that proves it. */
export interface Violation {
  rule: string;
  severity: Severity;
  type: 'forbidden' | 'allowed' | 'required';
  /** dependency: a direct import; reachable: through a chain; unreachable: no from-module reaches it; circular: it
   * leads back to its source; required: the module lacks the dependency. */
  kind: 'dependency' | 'reachable' | 'unreachable' | 'circular' | 'required';
  from: string;
  to: string;
  /** From `from` to `to` (a cycle: back to `from`), each step with its import line and kinds. */
  chain: Hop[];
  /** circular: the modules of the cycle after `from`, ending at `from`. */
  cycle?: string[];
  comment?: string;
  /** Listed in the baseline: reported, never failing. */
  known?: true;
}

/** A rules file that cannot be evaluated: every problem with its place in the file, and near names for a typo. */
export class RulesError extends Error {
  readonly problems: string[];
  readonly candidates: string[];
  constructor(problems: string[], candidates: string[] = []) {
    super(problems.join('\n'));
    this.problems = problems;
    this.candidates = candidates;
  }
}

/** The rule `allowed` violations carry, as dependency-cruiser names it. */
export const NOT_IN_ALLOWED = 'not-in-allowed';

// The JSON Schema subset scheme/rules.json uses: $ref, anyOf, enum, type, properties, required,
// additionalProperties: false, items, minItems, minLength.
interface Schema {
  $ref?: string;
  anyOf?: Schema[];
  enum?: string[];
  type?: 'object' | 'array' | 'string' | 'boolean';
  properties?: Record<string, Schema>;
  required?: string[];
  additionalProperties?: boolean;
  items?: Schema;
  minItems?: number;
  minLength?: number;
}
const DEFS = (schema as { $defs: Record<string, Schema> }).$defs;
const A = {
  object: 'an object',
  array: 'an array',
  string: 'a string',
  boolean: 'true or false',
};
const kindOf = (value: unknown) =>
  Array.isArray(value) ? 'array' : value === null ? 'null' : typeof value;

function validate(s: Schema, value: unknown, at: string, out: string[]) {
  const here = at || 'the rules file';
  if (s.$ref) validate(DEFS[s.$ref.replace('#/$defs/', '')], value, at, out);
  if (s.anyOf) {
    if (s.anyOf.every(branch => problemsOf(branch, value, at).length))
      out.push(
        `${here} must be ${s.anyOf.map(b => A[b.type ?? 'string']).join(' or ')}`
      );
    return;
  }
  if (s.enum && !s.enum.includes(value as string)) {
    out.push(`${here} must be one of ${s.enum.join(', ')}`);
    return;
  }
  if (s.type && kindOf(value) !== s.type) {
    out.push(`${here} must be ${A[s.type]}`);
    return;
  }
  if (typeof value === 'string' && value.length < (s.minLength ?? 0))
    out.push(`${here} must not be empty`);
  if (Array.isArray(value)) {
    if (value.length < (s.minItems ?? 0)) out.push(`${here} must not be empty`);
    if (s.items)
      value.forEach((item, i) => validate(s.items!, item, `${at}[${i}]`, out));
  }
  if (s.type === 'object' && kindOf(value) === 'object') {
    const object = value as Record<string, unknown>;
    for (const name of s.required ?? [])
      if (!(name in object)) out.push(`${here}: missing ${name}`);
    for (const [name, item] of Object.entries(object)) {
      const property = s.properties?.[name];
      if (property) validate(property, item, at ? `${at}.${name}` : name, out);
      else if (s.additionalProperties === false)
        out.push(`${here}: unknown property ${name}`);
    }
  }
}
function problemsOf(s: Schema, value: unknown, at: string) {
  const out: string[] = [];
  validate(s, value, at, out);
  return out;
}

// A compiled rule: its place in the file, matchers and the templates that take the from-module's groups.
interface Rule {
  type: Violation['type'];
  name: string;
  severity: Severity;
  comment?: string;
  load: RuleLoad;
  where: string;
  from: Matcher;
  to: {
    path?: Template;
    pathNot?: Template;
    reachable?: boolean;
    circular?: boolean;
    via?: Matcher;
    viaOnly?: Matcher;
    types?: string[];
    typesNot?: string[];
  };
}
interface Matcher {
  path?: RegExp;
  pathNot?: RegExp;
  /** `path` as written (an array joined with |). */
  text?: string;
  where: string;
}
interface Template {
  source: string;
  where: string;
  compiled: Map<string, RegExp>;
}

/** A parsed rules file: the compiled rules and the severity of `not-in-allowed`. */
export interface RuleSet {
  rules: Rule[];
  allowedSeverity: Severity;
}

const sourceOf = (p: Pattern) => (Array.isArray(p) ? p.join('|') : p);
const GROUP = /\$(\d)/g;
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const groupsIn = (re: RegExp) =>
  (new RegExp(`${re.source}|`).exec('')?.length ?? 1) - 1;

/** Parse a rules file (scheme/rules.json); every problem is a RulesError naming its place in the file. */
export function parseRules(value: unknown): RuleSet {
  const problems = problemsOf(schema as Schema, value, '');
  if (problems.length) throw new RulesError(problems);
  const file = value as RulesFile;
  const rules: Rule[] = [];
  const regex = (p: Pattern | undefined, where: string) => {
    if (p === undefined) return undefined;
    try {
      return new RegExp(sourceOf(p));
    } catch (error) {
      problems.push(
        `${where}: invalid regular expression (${error instanceof Error ? error.message : String(error)})`
      );
      return undefined;
    }
  };
  const matcher = (m: Modules | undefined, where: string): Matcher => ({
    path: regex(m?.path, `${where}.path`),
    pathNot: regex(m?.pathNot, `${where}.pathNot`),
    ...(m?.path && { text: sourceOf(m.path) }),
    where,
  });
  // A template compiles once per set of captured groups; the shape (each $N as any text) must compile now.
  const template = (
    p: Pattern | undefined,
    where: string,
    from: Matcher,
    fromWhere: string
  ): Template | undefined => {
    if (p === undefined) return undefined;
    const source = sourceOf(p);
    const groups = from.path ? groupsIn(from.path) : 0;
    for (const [, n] of source.matchAll(GROUP))
      if (Number(n) > groups)
        problems.push(
          `${where} uses $${n} but ${fromWhere}.path has ${groups} group${groups === 1 ? '' : 's'}`
        );
    if (!regex(source.replace(GROUP, '.*'), where)) return undefined;
    return { source, where, compiled: new Map() };
  };
  const named = new Map<string, string>();
  const name = (n: string, where: string) => {
    if (n === NOT_IN_ALLOWED)
      problems.push(
        `${where}.name ${n} is reserved for dependencies no allowed rule matches`
      );
    else if (named.has(n))
      problems.push(
        `rule name ${n} is used twice (${named.get(n)} and ${where}): baselines and SARIF key violations by name`
      );
    named.set(n, where);
    return n;
  };
  file.forbidden?.forEach((r, i) => {
    const where = `forbidden[${i}]`;
    const from = matcher(r.from, `${where}.from`);
    const to = r.to;
    if (to.reachable !== undefined && to.circular)
      problems.push(`${where}.to: reachable and circular are two rules`);
    if ((to.via || to.viaOnly) && !to.circular)
      problems.push(`${where}.to: via and viaOnly need circular: true`);
    if (
      to.reachable !== undefined &&
      (to.dependencyTypes || to.dependencyTypesNot)
    )
      problems.push(
        `${where}.to: a reachable rule walks the imports its load keeps; dependencyTypes apply to direct and circular rules`
      );
    if (
      to.reachable === false &&
      [to.path, to.pathNot].some(p => p && sourceOf(p).match(GROUP))
    )
      problems.push(
        `${where}.to: reachable: false has no single from-module to take $1…$9 from`
      );
    rules.push({
      type: 'forbidden',
      name: name(r.name, where),
      severity: r.severity ?? 'warn',
      ...(r.comment && { comment: r.comment }),
      load: r.load ?? 'runtime',
      where,
      from,
      to: {
        path: template(to.path, `${where}.to.path`, from, `${where}.from`),
        pathNot: template(
          to.pathNot,
          `${where}.to.pathNot`,
          from,
          `${where}.from`
        ),
        reachable: to.reachable,
        circular: to.circular,
        via: to.via && matcher(to.via, `${where}.to.via`),
        viaOnly: to.viaOnly && matcher(to.viaOnly, `${where}.to.viaOnly`),
        types: to.dependencyTypes,
        typesNot: to.dependencyTypesNot,
      },
    });
  });
  file.allowed?.forEach((r, i) => {
    const where = `allowed[${i}]`;
    const from = matcher(r.from, `${where}.from`);
    rules.push({
      type: 'allowed',
      name: NOT_IN_ALLOWED,
      severity: file.allowedSeverity ?? 'warn',
      ...(r.comment && { comment: r.comment }),
      load: r.load ?? 'runtime',
      where,
      from,
      to: {
        path: template(r.to.path, `${where}.to.path`, from, `${where}.from`),
        pathNot: template(
          r.to.pathNot,
          `${where}.to.pathNot`,
          from,
          `${where}.from`
        ),
        types: r.to.dependencyTypes,
        typesNot: r.to.dependencyTypesNot,
      },
    });
  });
  file.required?.forEach((r, i) => {
    const where = `required[${i}]`;
    const from = matcher(r.module, `${where}.module`);
    rules.push({
      type: 'required',
      name: name(r.name, where),
      severity: r.severity ?? 'warn',
      ...(r.comment && { comment: r.comment }),
      load: r.load ?? 'runtime',
      where,
      from,
      to: {
        path: template(r.to.path, `${where}.to.path`, from, `${where}.module`),
        reachable: r.to.reachable,
      },
    });
  });
  if (problems.length) throw new RulesError(problems);
  return { rules, allowedSeverity: file.allowedSeverity ?? 'warn' };
}

// The graph as one load sees it: the walked index, and the installed packages each file imports under that load.
interface View {
  R: QueryIndex;
  packages: Map<string, { id: string; kinds: string[] }[]>;
  pairs: Set<string>;
}
const pairKey = (u: string, v: string) => `${u}\0${v}`;

function viewOf(D: QueryData, load: RuleLoad): View {
  const R = indexOf(D, { load: load === 'all' ? null : load });
  const packages = new Map<string, { id: string; kinds: string[] }[]>();
  for (const x of D.externals ?? [])
    for (const file of new Set(x.files ?? [])) {
      if (x.testFiles?.includes(file) || !R.file.has(file)) continue;
      const kinds = x.kinds?.[file] ?? [];
      if (load !== 'all' && !keeps(load, loadOf(kinds, x.loads))) continue;
      const list = packages.get(file) ?? [];
      list.push({ id: x.id, kinds });
      packages.set(file, list);
    }
  const pairs = new Set<string>();
  for (const [u, list] of packages)
    for (const p of list) pairs.add(pairKey(u, p.id));
  for (let i = 0; i < R.G.n; i++)
    for (let k = R.G.out.offsets[i]; k < R.G.out.offsets[i + 1]; k++)
      pairs.add(pairKey(R.ids[i], R.ids[R.G.out.targets[k]]));
  return { R, packages, pairs };
}

// A file's direct dependencies under a view: files of the graph, then installed packages, each with its import kinds.
function* dependencies(V: View, u: string) {
  const i = V.R.G.index.get(u);
  if (i !== undefined)
    for (let k = V.R.G.out.offsets[i]; k < V.R.G.out.offsets[i + 1]; k++) {
      const v = V.R.ids[V.R.G.out.targets[k]];
      yield { to: v, kinds: V.R.evidence().get(pairKey(u, v))?.kinds ?? [] };
    }
  for (const p of V.packages.get(u) ?? []) yield { to: p.id, kinds: p.kinds };
}

const matches = (m: Matcher, id: string) =>
  (!m.path || m.path.test(id)) && (!m.pathNot || !m.pathNot.test(id));

// A template with the from-module's groups put in (escaped: a group is text, not a pattern).
function compiled(t: Template, groups: readonly string[]) {
  const source = t.source.replace(GROUP, (_, n: string) =>
    escape(groups[Number(n)] ?? '')
  );
  let re = t.compiled.get(source);
  if (!re) t.compiled.set(source, (re = new RegExp(source)));
  return re;
}
const toMatches = (r: Rule, id: string, groups: readonly string[]) =>
  (!r.to.path || compiled(r.to.path, groups).test(id)) &&
  (!r.to.pathNot || !compiled(r.to.pathNot, groups).test(id));
const typesMatch = (r: Rule, kinds: readonly string[]) =>
  (!r.to.types || kinds.some(k => r.to.types!.includes(k))) &&
  (!r.to.typesNot || !kinds.some(k => r.to.typesNot!.includes(k)));
const groupsOf = (r: Rule, id: string) =>
  r.from.path ? [...(r.from.path.exec(id) ?? [])] : [];

const SEVERITY: Record<Severity, number> = { error: 0, warn: 1, info: 2 };
const byText = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

export interface CheckOptions {
  /** A path that matches nothing is an error (default). Off for a base graph, which may predate what a rule names. */
  strict?: boolean;
}

/** Every violation of the rules in graph `D`, errors first, then by rule, source and target. */
export function checkRules(
  D: QueryData,
  set: RuleSet,
  { strict = true }: CheckOptions = {}
): Violation[] {
  const views = new Map<RuleLoad, View>();
  const view = (load: RuleLoad) => {
    let v = views.get(load);
    if (!v) views.set(load, (v = viewOf(D, load)));
    return v;
  };
  const files = indexOf(D).ids;
  const packages = (D.externals ?? []).map(x => x.id);
  if (strict) unmatched(set, files, packages);
  const out: Violation[] = [];
  for (const rule of set.rules) {
    if (rule.type === 'allowed') continue;
    const V = view(rule.load);
    const add = (
      v: Omit<Violation, 'rule' | 'severity' | 'type' | 'comment'>
    ) =>
      out.push({
        rule: rule.name,
        severity: rule.severity,
        type: rule.type,
        ...v,
        ...(rule.comment && { comment: rule.comment }),
      });
    if (rule.type === 'required') required(V, rule, files, add);
    else if (rule.to.circular) circular(V, rule, files, add);
    else if (rule.to.reachable === false)
      unreachable(V, rule, files, packages, add);
    else if (rule.to.reachable) reachable(V, rule, files, add);
    else
      for (const u of files.filter(f => matches(rule.from, f))) {
        const groups = groupsOf(rule, u);
        for (const d of dependencies(V, u))
          if (toMatches(rule, d.to, groups) && typesMatch(rule, d.kinds))
            add({
              kind: 'dependency',
              from: u,
              to: d.to,
              chain: chainHops(V.R, [u, d.to]),
            });
      }
  }
  notAllowed(set, view, files, out);
  return out.sort(
    (a, b) =>
      SEVERITY[a.severity] - SEVERITY[b.severity] ||
      byText(a.rule, b.rule) ||
      byText(a.from, b.from) ||
      byText(a.to, b.to)
  );
}

type Add = (
  v: Omit<Violation, 'rule' | 'severity' | 'type' | 'comment'>
) => void;

// Everything one file reaches under a view: files with the walk's parents, and each installed package with the
// nearest file that imports it.
function reachedFrom(V: View, seeds: string[]) {
  const walked = walk(V.R, seeds, { dir: 'out', maxDepth: Infinity });
  const files = new Map<string, number>();
  for (const [v, d] of walked.depth) files.set(V.R.ids[v], d);
  const packages = new Map<string, { importer: string; depth: number }>();
  for (const [file, d] of [...files].sort(
    (a, b) => a[1] - b[1] || byText(a[0], b[0])
  ))
    for (const p of V.packages.get(file) ?? [])
      if (!packages.has(p.id)) packages.set(p.id, { importer: file, depth: d });
  const chainOf = (id: string) => {
    const ids = [id];
    for (let v = V.R.G.index.get(id); v !== undefined;) {
      const parent = walked.via.get(v)?.[0];
      if (parent === undefined) break;
      ids.push(V.R.ids[parent]);
      v = parent;
    }
    return ids.reverse();
  };
  return { files, packages, chainOf };
}

function reachable(V: View, rule: Rule, files: string[], add: Add) {
  for (const u of files.filter(f => matches(rule.from, f))) {
    const groups = groupsOf(rule, u);
    const reached = reachedFrom(V, [u]);
    for (const v of reached.files.keys())
      if (v !== u && toMatches(rule, v, groups))
        add({
          kind: 'reachable',
          from: u,
          to: v,
          chain: chainHops(V.R, reached.chainOf(v)),
        });
    for (const [p, { importer }] of reached.packages)
      if (toMatches(rule, p, groups))
        add({
          kind: 'reachable',
          from: u,
          to: p,
          chain: chainHops(V.R, [...reached.chainOf(importer), p]),
        });
  }
}

function unreachable(
  V: View,
  rule: Rule,
  files: string[],
  packages: string[],
  add: Add
) {
  const seeds = files.filter(f => matches(rule.from, f));
  const reached = reachedFrom(V, seeds);
  const from = new Set(seeds);
  for (const id of [...files, ...packages])
    if (
      !from.has(id) &&
      !reached.files.has(id) &&
      !reached.packages.has(id) &&
      toMatches(rule, id, [])
    )
      add({ kind: 'unreachable', from: id, to: id, chain: [{ id }] });
}

// A dependency u → v inside a strongly connected set closes a cycle: the shortest way back from v to u.
function circular(V: View, rule: Rule, files: string[], add: Add) {
  const { G } = V.R;
  const { comp, comps } = V.R.components();
  for (const u of files.filter(f => matches(rule.from, f))) {
    const s = G.index.get(u);
    if (s === undefined) continue;
    const members = comps[comp[s]];
    const groups = groupsOf(rule, u);
    // Toward u: a breadth-first walk over reversed edges inside u's set gives each member its next step to u.
    const next = new Map<number, number>([[s, s]]);
    for (let frontier = [s]; frontier.length;) {
      const following: number[] = [];
      for (const w of frontier)
        for (let k = G.in.offsets[w]; k < G.in.offsets[w + 1]; k++) {
          const x = G.in.targets[k];
          if (comp[x] !== comp[s] || next.has(x)) continue;
          next.set(x, w);
          following.push(x);
        }
      frontier = following;
    }
    for (let k = G.out.offsets[s]; k < G.out.offsets[s + 1]; k++) {
      const t = G.out.targets[k];
      if (comp[t] !== comp[s] || (members.length === 1 && t !== s)) continue;
      const v = V.R.ids[t];
      if (!toMatches(rule, v, groups)) continue;
      if (!typesMatch(rule, V.R.evidence().get(pairKey(u, v))?.kinds ?? []))
        continue;
      const ring: number[] = [t];
      while (ring.at(-1) !== s) ring.push(next.get(ring.at(-1)!)!);
      const cycle = ring.map(i => V.R.ids[i]);
      const via = rule.to.via,
        only = rule.to.viaOnly;
      if (via && !cycle.some(id => matches(via, id))) continue;
      if (only && !cycle.every(id => matches(only, id))) continue;
      add({
        kind: 'circular',
        from: u,
        to: v,
        cycle,
        chain: chainHops(V.R, [u, ...cycle]),
      });
    }
  }
}

function required(V: View, rule: Rule, files: string[], add: Add) {
  for (const m of files.filter(f => matches(rule.from, f))) {
    const groups = groupsOf(rule, m);
    const targets = rule.to.reachable
      ? (() => {
          const r = reachedFrom(V, [m]);
          return [...r.files.keys(), ...r.packages.keys()].filter(
            id => id !== m
          );
        })()
      : [...dependencies(V, m)].map(d => d.to);
    if (targets.some(id => toMatches(rule, id, groups))) continue;
    const to = rule.to.path
      ? rule.to.path.source.replace(GROUP, (_, n: string) =>
          escape(groups[Number(n)] ?? '')
        )
      : '';
    add({ kind: 'required', from: m, to, chain: [{ id: m }] });
  }
}

// Allowed rules: every dependency under the widest load they name must match one of them, under that rule's load.
function notAllowed(
  set: RuleSet,
  view: (load: RuleLoad) => View,
  files: string[],
  out: Violation[]
) {
  const allowed = set.rules.filter(r => r.type === 'allowed');
  if (!allowed.length) return;
  const loads = new Set(allowed.map(r => r.load));
  const widest = loads.has('all')
    ? 'all'
    : loads.has('runtime')
      ? 'runtime'
      : 'eager';
  const V = view(widest);
  for (const u of files)
    for (const d of dependencies(V, u)) {
      const ok = allowed.some(
        r =>
          matches(r.from, u) &&
          toMatches(r, d.to, groupsOf(r, u)) &&
          typesMatch(r, d.kinds) &&
          view(r.load).pairs.has(pairKey(u, d.to))
      );
      if (!ok)
        out.push({
          rule: NOT_IN_ALLOWED,
          severity: set.allowedSeverity,
          type: 'allowed',
          kind: 'dependency',
          from: u,
          to: d.to,
          chain: chainHops(V.R, [u, d.to]),
        });
    }
}

// A path pattern that matches no file or package of the graph is a misspelling until proven otherwise: refused, with
// the nearest names.
function unmatched(set: RuleSet, files: string[], packages: string[]) {
  const ids = [...files, ...packages];
  const problems: string[] = [];
  const candidates = new Set<string>();
  const dirs = new Set(
    files.flatMap(f =>
      f
        .split('/')
        .slice(0, -1)
        .map((_, i, parts) => `${parts.slice(0, i + 1).join('/')}/`)
    )
  );
  // Each problem names its own near names: a misspelled rule is never read as one that holds (forbidden) or that
  // nothing satisfies (allowed, required).
  const check = (
    re: RegExp | undefined,
    where: string,
    source: string | undefined,
    effect: string
  ) => {
    if (!re || ids.some(id => re.test(id))) return;
    const literal = (source ?? '')
      .replace(/^\^|\$$/g, '')
      .replace(/\\(.)/g, '$1');
    const near = nearest(literal, [...ids, ...dirs]).best.toSorted();
    for (const id of near) candidates.add(id);
    problems.push(
      `${where} ${source} matches no file or package of this graph: check the spelling (${effect})${near.length ? `; near: ${near.join(', ')}` : ''}`
    );
  };
  const EFFECT: Record<Violation['type'], string> = {
    forbidden: 'a forbidden rule over nothing always holds',
    allowed: 'an allowed rule over nothing allows nothing',
    required: 'a required rule over nothing fails every module',
  };
  for (const r of set.rules) {
    const effect = EFFECT[r.type];
    check(r.from.path, `${r.from.where}.path`, r.from.text, effect);
    for (const via of [r.to.via, r.to.viaOnly])
      if (via) check(via.path, `${via.where}.path`, via.text, effect);
    const to = r.to.path;
    if (to)
      check(
        new RegExp(to.source.replace(GROUP, '.*')),
        to.where,
        to.source,
        effect
      );
  }
  if (problems.length)
    throw new RulesError(problems, [...candidates].toSorted());
}

/** One known violation: a baseline lists it so it is reported but never fails. */
export interface BaselineEntry {
  rule: string;
  from: string;
  to: string;
}
export interface Baseline {
  violations: BaselineEntry[];
}

const keyOf = (v: BaselineEntry) => JSON.stringify([v.rule, v.from, v.to]);

/** The baseline that marks every one of these violations known. */
export function baselineOf(violations: readonly Violation[]): Baseline {
  const byKey = new Map(
    violations.map(v => [keyOf(v), { rule: v.rule, from: v.from, to: v.to }])
  );
  return {
    violations: [...byKey.values()].sort(
      (a, b) =>
        byText(a.rule, b.rule) || byText(a.from, b.from) || byText(a.to, b.to)
    ),
  };
}

/** A baseline file's content, checked: `{ "violations": [{ "rule", "from", "to" }] }`. */
export function parseBaseline(value: unknown): Baseline {
  const list = (value as { violations?: unknown } | null)?.violations;
  const ok =
    Array.isArray(list) &&
    list.every(
      e =>
        e &&
        typeof e === 'object' &&
        ['rule', 'from', 'to'].every(
          k => typeof (e as Record<string, unknown>)[k] === 'string'
        )
    );
  if (!ok)
    throw new RulesError([
      'the baseline must be { "violations": [{ "rule", "from", "to" }] }',
    ]);
  return value as Baseline;
}

/** Violations the baseline lists are marked known; `fixed` lists the baseline entries no longer violated. */
export function applyBaseline(
  violations: readonly Violation[],
  baseline: Baseline | null
) {
  const listed = new Set((baseline?.violations ?? []).map(keyOf));
  const seen = new Set(violations.map(keyOf));
  return {
    violations: violations.map(v =>
      listed.has(keyOf(v)) ? { ...v, known: true as const } : v
    ),
    fixed: (baseline?.violations ?? []).filter(e => !seen.has(keyOf(e))),
  };
}

/** The key of a violation in baselines and SARIF fingerprints: rule, source, target. */
export const violationKey = keyOf;
