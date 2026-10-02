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
  /** Zod too_big: the inclusive upper bound. */
  maximum?: number | bigint;
}

export interface FormattedIssue {
  message: string;
  path: PropertyKey[];
}

type JsonNode = Record<string, unknown>;

/** `["…`, `[{…` or `[[…`: an attempted JSON array, not a literal value. */
const JSON_ARRAY_TEXT = /^\s*\[\s*["{[]/;

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

/**
 * Commonly guessed field names mapped to the canonical field when the query
 * accepts it; the first accepted target wins. Same table and order as the
 * native validator's `FIELD_ALIASES` (the parity suite pins both).
 */
const FIELD_ALIASES: readonly (readonly [string, string])[] = [
  ['type', 'operation'],
  ['path', 'uri'],
  ['filePath', 'uri'],
  ['keywordsToSearch', 'keywords'],
  ['matchStringContextLines', 'contextLines'],
  ['pattern', 'searchText'],
  ['pattern', 'names'],
  ['filesOnly', 'resultView'],
  ['filePath', 'path'],
  ['maxResults', 'pageSize'],
  ['limit', 'pageSize'],
  ['depth', 'maxDepth'],
  ['lineStart', 'startLine'],
  ['lineEnd', 'endLine'],
  ['searchText', 'matchString'],
  ['filePattern', 'include'],
  ['fileFilter', 'include'],
  ['includePattern', 'include'],
  ['glob', 'include'],
  ['useRegex', 'regex'],
  ['isRegex', 'regex'],
  ['includeHidden', 'hidden'],
  ['showHidden', 'hidden'],
];

/**
 * The field `known` most likely meant by `unknown`: an alias, a known field
 * that prefixes it (`keywordsToSearch` → `keywords`), or the nearest spelling
 * within an edit budget of 2–3 (native `suggest_field`).
 */
export function suggestField(
  unknown: string,
  known: readonly string[]
): string | undefined {
  for (const [alias, target] of FIELD_ALIASES)
    if (alias === unknown && known.includes(target)) return target;
  const prefix = known
    .filter(k => k.length >= 4 && unknown.startsWith(k) && unknown !== k)
    .sort((a, b) => b.length - a.length)[0];
  if (prefix) return prefix;
  const budget = Math.min(3, Math.max(2, Math.floor([...unknown].length / 3)));
  const lowered = unknown.toLowerCase();
  let best: string | undefined;
  let bestScore = Infinity;
  for (const candidate of known) {
    const score = editDistance(lowered, candidate.toLowerCase());
    if (score <= budget && score < bestScore) {
      best = candidate;
      bestScore = score;
    }
  }
  return best;
}

/**
 * A boolean sent for an on/off enum (`regex:true`, `minify:false`): name the
 * value that means what was intended (native `boolean_enum_hint`).
 */
function booleanEnumHint(input: unknown, allowed: readonly string[]): string {
  if (typeof input !== 'boolean') return '';
  const off = ['none', 'literal', 'off'].find(name => allowed.includes(name));
  const pick = input ? allowed.find(value => value !== off) : off;
  return pick ? ` — booleans are not accepted; use "${pick}"` : '';
}

/** Native range wording: `(0-20)`, `(>= 1)`, or `(<= 20)`. */
function rangeMessage(minimum: unknown, maximum: unknown): string {
  const low = typeof minimum === 'number' ? minimum : undefined;
  const high = typeof maximum === 'number' ? maximum : undefined;
  const range =
    low !== undefined && high !== undefined
      ? `${low}-${high}`
      : low !== undefined
        ? `>= ${low}`
        : high !== undefined
          ? `<= ${high}`
          : '';
  return `Number is outside the allowed range (${range})`;
}

/** `queries[0].context`, or "the request" at the root (native wording). */
const location = (path: readonly PropertyKey[]) =>
  path.length
    ? path
        .map((key, position) =>
          typeof key === 'number'
            ? `[${key}]`
            : `${position ? '.' : ''}${String(key)}`
        )
        .join('')
    : 'the request';

const display = (value: unknown) =>
  typeof value === 'string' ? JSON.stringify(value) : String(value);

function enumMessage(input: unknown, allowed: readonly unknown[]): string {
  const values = [...new Set(allowed.map(String))];
  return (
    `Value ${display(input)} is outside the allowed enum; allowed: ` +
    `${values.join(', ')}${didYouMean(input, values)}${booleanEnumHint(input, values)}`
  );
}

/** The one value a `const` or single-value `enum` schema pins. */
const singleLiteral = (
  schema: JsonNode | undefined
): { value: unknown } | undefined => {
  if (!schema) return undefined;
  if ('const' in schema) return { value: schema.const };
  return Array.isArray(schema.enum) && schema.enum.length === 1
    ? { value: schema.enum[0] }
    : undefined;
};

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

  nodesAt(path: readonly PropertyKey[]): JsonNode[] {
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

  /**
   * When `key` is declared only by some branches at `path` (a sibling form),
   * how to reach a declaring form (native `annotate_sibling_branch_fields`):
   * the first declaring form's required fields `value` lacks, else the
   * literal selectors (`operation:"pullRequest"`) `value` sets differently,
   * one alternative per declaring form. Also the fields of the forms that do
   * not declare it. `undefined` when every branch or none declares it.
   */
  siblingForm(
    path: readonly PropertyKey[],
    key: string,
    value: unknown
  ): { requires: string[]; selectors: string[]; others: string[] } | undefined {
    const nodes = this.nodesAt(path).filter(node => node.properties);
    const declares = (node: JsonNode) =>
      Object.hasOwn(node.properties as JsonNode, key);
    const owners = nodes.filter(declares);
    const others = nodes.filter(node => !declares(node));
    if (!owners.length || !others.length) return undefined;
    const missing = (node: JsonNode) =>
      (Array.isArray(node.required) ? (node.required as unknown[]) : []).filter(
        (field): field is string =>
          typeof field === 'string' &&
          field !== key &&
          valueAt(value, [field]) === undefined
      );
    // One alternative per declaring form (`pattern or rule`), as native
    // `sibling_missing_fields` names them.
    const requires = [
      ...new Set(
        owners
          .map(missing)
          .filter(fields => fields.length)
          .map(fields => fields.join(' and '))
      ),
    ];
    const selectors = requires.length
      ? []
      : [
          ...new Set(
            owners
              .map(node =>
                Object.entries(node.properties as JsonNode)
                  .flatMap(([name, raw]) => {
                    const literal = singleLiteral(this.deref(raw));
                    const supplied = valueAt(value, [name]);
                    return literal &&
                      supplied !== undefined &&
                      supplied !== literal.value
                      ? [`${name}:${JSON.stringify(literal.value)}`]
                      : [];
                  })
                  .join(' and ')
              )
              .filter(Boolean)
          ),
        ];
    // The fields of the form actually sent, not of every other form.
    const sent = others.filter(node => this.selects(node, value));
    return {
      requires,
      selectors,
      others: [
        ...new Set(
          (sent.length ? sent : others).flatMap(node =>
            Object.keys(node.properties as JsonNode)
          )
        ),
      ],
    };
  }

  /** Whether `value`'s literal selectors (`operation`, …) pick `node`. */
  private selects(node: JsonNode, value: unknown): boolean {
    return Object.entries(node.properties as JsonNode).every(([key, raw]) => {
      const supplied = valueAt(value, [key]);
      const schema = this.deref(raw);
      if (supplied === undefined || !schema) return true;
      if ('const' in schema) return schema.const === supplied;
      if (Array.isArray(schema.enum)) return schema.enum.includes(supplied);
      return true;
    });
  }

  /** Field names valid at `path`, narrowed to the branches `value` selects. */
  fields(path: readonly PropertyKey[], value: unknown): string[] {
    const nodes = this.nodesAt(path).filter(node => node.properties);
    const selected = nodes.filter(node => this.selects(node, value));
    const chosen = selected.length ? selected : nodes;
    return [
      ...new Set(
        chosen.flatMap(node => Object.keys(node.properties as JsonNode))
      ),
    ];
  }
}

export interface IssueContext {
  /** The value the issue paths are relative to (after native normalization). */
  value: unknown;
  /** JSON Schema (input io) of the tool input, for valid-field lists. */
  jsonSchema: () => JsonNode | undefined;
}

/**
 * Fields set by copying a `next.*` continuation (or opt-in diagnostics), never
 * composed by hand: core `CONTINUATION_FIELDS`, which the published view also
 * leaves out. Every other canonical field stays listable even when the
 * published view hides it, so a rare field remains discoverable after a
 * mistake.
 */
const CONTINUATION_ONLY =
  /^(debug|(match|file|comment|commit|review|metadata|diagnostic)?[pP]age|(diagnostic)?[sS]napshot|cursor|(char|commentBody|node|materialize)Offset|response[A-Z]\w*)$/;

export function formatIssues(
  issues: readonly RawIssue[],
  context: IssueContext
): FormattedIssue[] {
  let index: FieldIndex | undefined | null = null;
  const canonical = () => {
    if (index === null) {
      const root = context.jsonSchema();
      index = root ? new FieldIndex(root) : undefined;
    }
    return index;
  };
  const composable = (fields: readonly string[]) =>
    fields.filter(field => !CONTINUATION_ONLY.test(field));
  const validFields = (path: readonly PropertyKey[], value: unknown) =>
    composable(canonical()?.fields(path, value) ?? []);
  const itemsAcceptString = (path: readonly PropertyKey[]) => {
    if (index === null) {
      const root = context.jsonSchema();
      index = root ? new FieldIndex(root) : undefined;
    }
    return (
      index
        ?.nodesAt([...path, 0])
        .some(node => node.type === undefined || node.type === 'string') ??
      false
    );
  };
  /** Allowed values of a union discriminator, gathered across every branch. */
  const discriminators = new Map<string, unknown[]>();
  const missingField = (
    name: PropertyKey | undefined,
    allowed: readonly unknown[]
  ) =>
    `Missing required field${name === undefined ? '' : `: ${String(name)}`} (one of: ${[...new Set(allowed.map(String))].join(', ')})`;
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
                ? missingField(path.at(-1), issue.options)
                : enumMessage(supplied, issue.options),
          });
          return;
        }
        if (issue.errors?.length) return visitUnion(issue, path);
        break;
      case 'unrecognized_keys': {
        let valid = validFields(path, supplied);
        for (const key of issue.keys ?? []) {
          // A field another form of this union declares: the input mixes
          // shapes (e.g. clasify matrix fields beside queries[]). Native CLI
          // wording, plus the fields of the form actually sent.
          const sibling = canonical()?.siblingForm(path, key, supplied);
          if (sibling?.selectors.length) {
            // A field of another operation (or type/analysis/ruleKind): name
            // the selector values that declare it; the fields listed stay
            // those of the operation actually sent.
            out.push({
              path,
              message: `Remove '${key}' from ${location(path)}: it applies only with ${sibling.selectors.join(' or ')}.`,
            });
            continue;
          }
          if (sibling) {
            valid = composable(sibling.others);
            const needs = sibling.requires.length
              ? `: it applies only with ${sibling.requires.join(' or ')}`
              : '';
            out.push({
              path,
              message: `Remove '${key}' from ${location(path)}${needs} (send one shape)`,
            });
            continue;
          }
          // `goal`/`reasoning` beside `queries`: each row states its own.
          if (
            path.length === 0 &&
            (key === 'goal' || key === 'reasoning') &&
            supplied !== null &&
            typeof supplied === 'object' &&
            Object.hasOwn(supplied, 'queries')
          ) {
            out.push({
              path,
              message: `Move '${key}' into each queries[] row: a top-level ${key} is not inherited.`,
            });
            continue;
          }
          // Suggest from every field the sent form accepts (native
          // `knownFields`), including ones the advertised view leaves out.
          const guess = suggestField(
            key,
            canonical()?.fields(path, supplied) ?? valid
          );
          out.push({
            path,
            message: `Remove unknown field '${key}'${guess ? ` (did you mean '${guess}'?)` : ''}`,
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
      case 'too_small':
      case 'too_big': {
        if (typeof supplied === 'number') {
          const bounds = canonical()
            ?.nodesAt(path)
            .find(node => 'minimum' in node || 'maximum' in node);
          out.push({
            path,
            message: rangeMessage(bounds?.minimum, bounds?.maximum),
          });
          return;
        }
        if (issue.code === 'too_small') break;
        const rows = Array.isArray(supplied) ? supplied.length : undefined;
        const maximum = Number(issue.maximum);
        if (
          path.length === 1 &&
          path[0] === 'queries' &&
          rows !== undefined &&
          maximum > 0
        ) {
          out.push({ path, message: issue.message });
          out.push({
            path,
            message: `Send at most ${maximum} rows per call: split the batch into ${Math.ceil(rows / maximum)} calls.`,
          });
          return;
        }
        break;
      }
      case 'invalid_value':
        if (issue.values?.length) {
          const allowed = discriminators.get(path.join('.')) ?? issue.values;
          out.push({
            path,
            message:
              supplied === undefined
                ? missingField(path.at(-1), allowed)
                : enumMessage(supplied, allowed),
          });
          return;
        }
        break;
      case 'invalid_type':
        // Native normalization already turned JSON-encoded lists and scalars
        // the items accept into arrays; name the fix for what is left, and
        // never suggest wrapping an encoded list or an item the list rejects.
        if (issue.expected === 'array' && typeof supplied === 'string') {
          if (path.length === 1 && path[0] === 'queries') {
            out.push({
              path,
              message:
                'Expected an array of query objects; send queries as a JSON array, not a string',
            });
            return;
          }
          out.push({
            path,
            message: JSON_ARRAY_TEXT.test(supplied)
              ? 'Expected array; send a JSON array, not a JSON-encoded string'
              : itemsAcceptString(path)
                ? `Expected array; wrap the value: ${JSON.stringify([supplied])}`
                : 'Expected array',
          });
          return;
        }
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
    for (const branch of branches)
      for (const entry of branch)
        if (entry.code === 'invalid_value' && entry.path?.length === 1) {
          const key = [...path, segment(entry.path[0]!)].join('.');
          discriminators.set(key, [
            ...(discriminators.get(key) ?? []),
            ...(entry.values ?? []),
          ]);
        }
    // Two branches that each reject only the other's root fields: the input
    // mixes forms (clasify preset + custom question, context value + tool).
    const unknownOnly = branches
      .map(branch =>
        branch.every(
          entry => entry.code === 'unrecognized_keys' && !entry.path?.length
        )
          ? branch.flatMap(entry => [...(entry.keys ?? [])])
          : []
      )
      .filter(keys => keys.length);
    for (const [index, left] of unknownOnly.entries()) {
      const right = unknownOnly
        .slice(index + 1)
        .find(keys => !keys.some(key => left.includes(key)));
      if (right) {
        const quote = (keys: string[]) => keys.map(k => `\`${k}\``).join(', ');
        out.push({
          path,
          message: `${quote(right)} and ${quote(left)} belong to different forms and cannot be combined; send one form`,
        });
        return;
      }
    }
    const candidates = branches.filter(
      branch => mismatches(branch).length === 0
    );
    // A discriminator the candidate branches do not even declare (e.g.
    // context.tool) selects the branch that pins it: report its allowed values.
    const undeclared = new Set(
      candidates.flatMap(branch =>
        branch
          .filter(
            entry => entry.code === 'unrecognized_keys' && !entry.path?.length
          )
          .flatMap(entry => [...(entry.keys ?? [])])
      )
    );
    const owned = branches
      .filter(branch => !candidates.includes(branch))
      .flatMap(mismatches)
      .filter(
        entry =>
          undeclared.has(String(segment(entry.path![0]!))) &&
          candidates.every(branch =>
            branch.some(
              other =>
                other.code === 'unrecognized_keys' &&
                !other.path?.length &&
                other.keys?.includes(String(segment(entry.path![0]!)))
            )
          )
      );
    if (candidates.length && owned.length) {
      for (const entry of owned) {
        const key = String(segment(entry.path![0]!));
        out.push({
          path: [...path, key],
          message: enumMessage(valueAt(row, [key]), entry.values ?? []),
        });
      }
      return;
    }
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
