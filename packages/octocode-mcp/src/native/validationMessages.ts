/**
 * Rewrites Standard Schema (Zod) validation issues into the actionable wording
 * the native CLI uses, so an MCP client gets the same guidance for a malformed
 * call: allowed enum/discriminator values, the nearest valid field for an
 * unknown key, plainly named missing fields, and the valid field list.
 * Only issue text changes; which inputs pass or fail is untouched.
 */

type PathSegment = PropertyKey | { key: PropertyKey };

export interface RawIssue {
  code?: string;
  message: string;
  path?: readonly PathSegment[];
  values?: readonly unknown[];
  keys?: readonly string[];
  expected?: string;
  errors?: readonly (readonly RawIssue[])[];
  /** Zod discriminatedUnion: the allowed discriminator values. */
  options?: readonly unknown[];
}

export interface FormattedIssue {
  message: string;
  path: PropertyKey[];
}

type JsonNode = Record<string, unknown>;

const segment = (p: PathSegment): PropertyKey =>
  typeof p === 'object' ? p.key : (p as PropertyKey);

const valueAt = (value: unknown, path: readonly PropertyKey[]): unknown => {
  let current = value;
  for (const key of path) {
    if (current === null || typeof current !== 'object') return undefined;
    current = (current as Record<PropertyKey, unknown>)[key];
  }
  return current;
};

export function editDistance(a: string, b: string): number {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    let diagonal = row[0]!;
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const above = row[j]!;
      row[j] = Math.min(
        above + 1,
        row[j - 1]! + 1,
        diagonal + (a[i - 1] === b[j - 1] ? 0 : 1)
      );
      diagonal = above;
    }
  }
  return row[b.length]!;
}

export function nearest(
  input: string,
  candidates: readonly string[]
): string | undefined {
  const lowered = input.toLowerCase();
  let best: string | undefined;
  let bestScore = Infinity;
  for (const candidate of candidates) {
    const score =
      candidate.toLowerCase() === lowered
        ? 0
        : editDistance(lowered, candidate.toLowerCase());
    if (score < bestScore) {
      best = candidate;
      bestScore = score;
    }
  }
  const limit = Math.max(2, Math.floor(input.length / 3));
  return bestScore <= limit ? best : undefined;
}

const didYouMean = (input: unknown, candidates: readonly string[]) => {
  if (typeof input !== 'string') return '';
  const guess = nearest(input, candidates);
  return guess && guess !== input ? ` (did you mean '${guess}'?)` : '';
};

const display = (value: unknown) =>
  typeof value === 'string' ? JSON.stringify(value) : String(value);

function enumMessage(input: unknown, allowed: readonly unknown[]): string {
  const values = [...new Set(allowed.map(String))];
  return (
    `Value ${display(input)} is outside the allowed enum; allowed: ` +
    `${values.join(', ')}${didYouMean(input, values)}`
  );
}

/** Minimal JSON-Schema walker used only to name valid fields at a path. */
class FieldIndex {
  constructor(private readonly root: JsonNode) {}

  private deref(node: unknown): JsonNode | undefined {
    if (!node || typeof node !== 'object') return undefined;
    const ref = (node as JsonNode).$ref;
    if (typeof ref === 'string' && ref.startsWith('#/')) {
      const target = valueAt(
        this.root,
        ref.slice(2).split('/').map(decodeURIComponent)
      );
      return this.deref(target);
    }
    return node as JsonNode;
  }

  /** Leaf alternatives of a node (anyOf/oneOf flattened, allOf merged in). */
  private branches(node: unknown): JsonNode[] {
    const resolved = this.deref(node);
    if (!resolved) return [];
    const alternatives = [
      ...((resolved.anyOf as unknown[]) ?? []),
      ...((resolved.oneOf as unknown[]) ?? []),
    ];
    const own = alternatives.length
      ? alternatives.flatMap(child => this.branches(child))
      : [resolved];
    const shared = ((resolved.allOf as unknown[]) ?? []).flatMap(child =>
      this.branches(child)
    );
    return shared.length
      ? own.map(branch => ({
          ...branch,
          properties: Object.assign(
            {},
            ...shared.map(s => s.properties ?? {}),
            branch.properties ?? {}
          ),
        }))
      : own;
  }

  private nodesAt(path: readonly PropertyKey[]): JsonNode[] {
    let nodes = this.branches(this.root);
    for (const key of path) {
      nodes = nodes.flatMap(node => {
        if (typeof key === 'number') {
          const items = node.items ?? node.prefixItems;
          return this.branches(
            Array.isArray(items) ? (items[key] ?? items[0]) : items
          );
        }
        const properties = node.properties as JsonNode | undefined;
        return properties && key in properties
          ? this.branches(properties[key as string])
          : [];
      });
    }
    return nodes;
  }

  /** Field names valid at `path`, narrowed to the branches `value` selects. */
  fields(path: readonly PropertyKey[], value: unknown): string[] {
    const nodes = this.nodesAt(path).filter(node => node.properties);
    const selects = (node: JsonNode) =>
      Object.entries(node.properties as JsonNode).every(([key, raw]) => {
        const supplied = valueAt(value, [key]);
        const schema = this.deref(raw);
        if (supplied === undefined || !schema) return true;
        if ('const' in schema) return schema.const === supplied;
        if (Array.isArray(schema.enum)) return schema.enum.includes(supplied);
        return true;
      });
    const selected = nodes.filter(selects);
    const chosen = selected.length ? selected : nodes;
    return [
      ...new Set(
        chosen.flatMap(node => Object.keys(node.properties as JsonNode))
      ),
    ];
  }
}

export interface IssueContext {
  /** The value the issue paths are relative to (after bare-query wrapping). */
  value: unknown;
  /** JSON Schema (input io) of the tool input, for valid-field lists. */
  jsonSchema: () => JsonNode | undefined;
}

export function formatIssues(
  issues: readonly RawIssue[],
  context: IssueContext
): FormattedIssue[] {
  let index: FieldIndex | undefined | null = null;
  const fieldIndex = () => {
    if (index === null) {
      const root = context.jsonSchema();
      index = root ? new FieldIndex(root) : undefined;
    }
    return index;
  };
  const out: FormattedIssue[] = [];
  const pointers = new Map<string, FormattedIssue>();

  const visit = (issue: RawIssue, base: readonly PropertyKey[]) => {
    const path = [...base, ...(issue.path ?? []).map(segment)];
    const supplied = valueAt(context.value, path);
    switch (issue.code) {
      case 'invalid_union':
        if (issue.options?.length) {
          out.push({
            path,
            message:
              supplied === undefined
                ? `Missing required field: ${String(path.at(-1))}; allowed: ${issue.options.map(String).join(', ')}`
                : enumMessage(supplied, issue.options),
          });
          return;
        }
        if (issue.errors?.length) return visitUnion(issue, path);
        break;
      case 'unrecognized_keys': {
        const valid = fieldIndex()?.fields(path, supplied) ?? [];
        for (const key of issue.keys ?? []) {
          out.push({
            path,
            message: `Remove unknown field '${key}'${didYouMean(key, valid)}`,
          });
        }
        if (valid.length && !pointers.has(path.join('.'))) {
          pointers.set(path.join('.'), {
            path,
            message: `Valid fields: ${valid.join(', ')}`,
          });
        }
        return;
      }
      case 'invalid_value':
        if (issue.values?.length) {
          out.push({ path, message: enumMessage(supplied, issue.values) });
          return;
        }
        break;
      case 'invalid_type':
        if (supplied === undefined) {
          const name = path.at(-1);
          // Keep core-authored guidance (e.g. "Set path to a local file.").
          const hint = issue.message.startsWith('Invalid input')
            ? ''
            : ` (${issue.message})`;
          out.push({
            path,
            message: `Missing required field${
              name === undefined ? '' : `: ${String(name)}`
            }${hint}`,
          });
          return;
        }
        break;
    }
    out.push({ path, message: issue.message });
  };

  const visitUnion = (issue: RawIssue, path: PropertyKey[]) => {
    const branches = issue.errors ?? [];
    const row = valueAt(context.value, path);
    // A discriminator mismatch: the caller supplied a value for a top-level
    // key that this branch pins to other literal value(s).
    const mismatches = (branch: readonly RawIssue[]) =>
      branch.filter(
        entry =>
          entry.code === 'invalid_value' &&
          entry.path?.length === 1 &&
          valueAt(row, [segment(entry.path[0]!)]) !== undefined
      );
    const custom =
      issue.message && issue.message !== 'Invalid input'
        ? issue.message
        : undefined;
    const candidates = branches.filter(
      branch => mismatches(branch).length === 0
    );
    if (candidates.length === 0) {
      const perBranch = branches.map(branch =>
        mismatches(branch).map(entry => String(segment(entry.path![0]!)))
      );
      const shared = perBranch[0]!.filter(key =>
        perBranch.every(keys => keys.includes(key))
      );
      if (shared.length) {
        for (const key of shared) {
          const allowed = branches.flatMap(branch =>
            branch
              .filter(
                entry =>
                  entry.code === 'invalid_value' &&
                  entry.path?.length === 1 &&
                  String(segment(entry.path[0]!)) === key
              )
              .flatMap(entry => entry.values ?? [])
          );
          out.push({
            path: [...path, key],
            message: enumMessage(valueAt(row, [key]), allowed),
          });
        }
        return;
      }
    }
    const pool = candidates.length ? candidates : branches;
    const fewest = Math.min(...pool.map(branch => branch.length));
    const best = pool.filter(branch => branch.length === fewest);
    const keyOf = (entry: RawIssue) =>
      `${entry.code}:${(entry.path ?? []).map(segment).join('.')}`;
    const common = best[0]!.filter(entry =>
      best.every(branch => branch.some(other => keyOf(other) === keyOf(entry)))
    );
    if (best.length > 1 && common.length < best[0]!.length) {
      // Several equally-close variants (e.g. astSearch match by pattern or
      // rule): report what they share, then the alternatives.
      for (const entry of common) visit(entry, path);
      const alternatives = best.map(branch =>
        branch
          .filter(
            entry =>
              !common.includes(entry) &&
              !common.some(c => keyOf(c) === keyOf(entry))
          )
          .map(
            entry => (entry.path ?? []).map(segment).join('.') || entry.message
          )
          .join(' + ')
      );
      out.push({
        path,
        message: `Provide one of: ${[...new Set(alternatives)].join(' | ')}`,
      });
    } else {
      for (const entry of best[0]!) visit(entry, path);
    }
    if (custom) out.push({ path, message: custom });
  };

  for (const issue of issues) visit(issue, []);
  const seen = new Set<string>();
  return [...out, ...pointers.values()].filter(issue => {
    const key = `${issue.path.join('.')}\u0000${issue.message}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
