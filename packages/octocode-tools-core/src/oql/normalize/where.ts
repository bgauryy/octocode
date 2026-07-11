/* ------------------------------- where ---------------------------------- */
import { PredicateSchema } from '../schema.js';
import { diagnostic } from '../diagnostics.js';
import { countPredicateNodes } from '../predicateUtils.js';
import type {
  OqlInputQuery,
  OqlQuery,
  Predicate,
  StructuralRuleInput,
} from '../types.js';
import { fail, formatZodError } from './shared.js';

export function normalizeWhere(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): Predicate | undefined {
  const sugarPredicate = buildSugarPredicate(raw);

  if (raw.where && sugarPredicate) {
    fail(
      diagnostic(
        'ambiguousSugar',
        'Provide either a canonical `where` predicate or top-level match sugar, not both.',
        { queryPath: 'where' }
      )
    );
  }

  let predicate = raw.where ?? sugarPredicate;

  // `filesWithoutMatch` sugar becomes target:"files" + not(predicate). The
  // target flip is handled above; here we only wrap the predicate.
  if (raw.filesWithoutMatch && predicate) {
    predicate = { kind: 'not', predicate };
  }

  // invert sugar wraps the whole predicate
  if (raw.invert === true && predicate) {
    predicate = { kind: 'not', predicate };
  }

  if (!predicate) {
    if (target === 'code') return undefined; // caller raises invalidQuery
    return undefined;
  }

  validatePredicate(predicate, 'where');
  validatePredicateIds(predicate, 'where', new Map());

  // Validate via schema (catches malformed shapes/unknown keys inside where).
  const check = PredicateSchema.safeParse(predicate);
  if (!check.success) {
    fail(
      diagnostic('invalidQuery', formatZodError(check.error), {
        queryPath: 'where',
      })
    );
  }
  return check.data as Predicate;
}

/** Default mirrors DEFAULTS.maxBooleanExpansion (kept local to avoid a cycle). */
const DEFAULT_MAX_BOOLEAN_EXPANSION = 64;

function booleanExpansionBudget(raw: OqlInputQuery): number {
  const b = (
    raw.controls as { budget?: { maxBooleanExpansion?: number } } | undefined
  )?.budget?.maxBooleanExpansion;
  return typeof b === 'number' && b > 0 ? b : DEFAULT_MAX_BOOLEAN_EXPANSION;
}

/**
 * Build a predicate from top-level match sugar. Boolean sugar (and/or/xor/
 * noneOf/oneOf) is normalized to canonical all/any/not here. Expansions that
 * generate more nodes than `controls.budget.maxBooleanExpansion` fail with
 * `budgetExhausted` rather than ballooning the plan.
 */
function buildSugarPredicate(raw: OqlInputQuery): Predicate | undefined {
  const checkBudget = (p: Predicate): Predicate => {
    const nodes = countPredicateNodes(p);
    const budget = booleanExpansionBudget(raw);
    if (nodes > budget) {
      fail(
        diagnostic(
          'budgetExhausted',
          `Boolean sugar expanded to ${nodes} predicate nodes, over controls.budget.maxBooleanExpansion (${budget}). Narrow the query or raise the budget.`,
          { queryPath: 'where' }
        )
      );
    }
    return p;
  };
  // boolean sugar
  if (Array.isArray(raw.and)) {
    return { kind: 'all', of: raw.and.map(coercePredicate) };
  }
  if (Array.isArray(raw.or)) {
    return { kind: 'any', of: raw.or.map(coercePredicate) };
  }
  if (Array.isArray(raw.noneOf)) {
    return {
      kind: 'not',
      predicate: { kind: 'any', of: raw.noneOf.map(coercePredicate) },
    };
  }
  if (Array.isArray(raw.xor)) {
    if (raw.xor.length !== 2) {
      fail(
        diagnostic(
          'invalidQuery',
          'xor is binary; use oneOf for multi-way exclusive matching.',
          { queryPath: 'xor' }
        )
      );
    }
    const a = coercePredicate(raw.xor[0]);
    const b = coercePredicate(raw.xor[1]);
    return checkBudget({
      kind: 'any',
      of: [
        { kind: 'all', of: [a, { kind: 'not', predicate: b }] },
        { kind: 'all', of: [{ kind: 'not', predicate: a }, b] },
      ],
    });
  }
  if (Array.isArray(raw.oneOf)) {
    return checkBudget(expandOneOf(raw.oneOf.map(coercePredicate)));
  }

  // structural sugar
  if (typeof raw.pattern === 'string' || raw.rule !== undefined) {
    if (typeof raw.pattern === 'string' && raw.rule !== undefined) {
      fail(
        diagnostic(
          'invalidQuery',
          'A structural predicate uses exactly one of `pattern` or `rule`.',
          { queryPath: 'pattern' }
        )
      );
    }
    if (typeof raw.lang !== 'string') {
      fail(
        diagnostic('invalidQuery', 'Structural sugar requires `lang`.', {
          queryPath: 'lang',
        })
      );
    }
    return {
      kind: 'structural',
      lang: raw.lang,
      ...(typeof raw.pattern === 'string' ? { pattern: raw.pattern } : {}),
      ...(raw.rule !== undefined
        ? { rule: raw.rule as StructuralRuleInput }
        : {}),
    };
  }

  if (typeof raw.text === 'string') {
    return { kind: 'text', value: raw.text };
  }
  if (typeof raw.regex === 'string') {
    return { kind: 'regex', value: raw.regex };
  }
  return undefined;
}

function coercePredicate(p: unknown): Predicate {
  if (p && typeof p === 'object' && 'kind' in (p as Record<string, unknown>)) {
    return p as Predicate;
  }
  fail(
    diagnostic(
      'invalidQuery',
      'Boolean sugar children must be predicate objects with a `kind`.',
      { queryPath: 'where' }
    )
  );
}

function expandOneOf(predicates: Predicate[]): Predicate {
  // exactly-one expansion: any( all(Pi, not(Pj!=i)) )
  const branches: Predicate[] = predicates.map((p, i) => {
    const negatives = predicates
      .filter((_, j) => j !== i)
      .map<Predicate>(q => ({ kind: 'not', predicate: q }));
    return { kind: 'all', of: [p, ...negatives] };
  });
  return { kind: 'any', of: branches };
}

function validatePredicate(p: Predicate, path: string): void {
  switch (p.kind) {
    case 'all':
    case 'any':
      if (!Array.isArray(p.of) || p.of.length === 0) {
        fail(
          diagnostic('invalidQuery', `Empty \`${p.kind}.of\` is invalid.`, {
            queryPath: path,
          })
        );
      }
      p.of.forEach((c, i) => validatePredicate(c, `${path}.of[${i}]`));
      break;
    case 'not':
      if (!p.predicate) {
        fail(
          diagnostic('invalidQuery', '`not` must contain exactly one child.', {
            queryPath: path,
          })
        );
      }
      validatePredicate(p.predicate, `${path}.predicate`);
      break;
    case 'structural':
      if ((typeof p.pattern === 'string') === (p.rule !== undefined)) {
        fail(
          diagnostic(
            'invalidQuery',
            'A structural predicate uses exactly one of `pattern` or `rule`.',
            { queryPath: path }
          )
        );
      }
      break;
    case 'field':
      validateFieldPredicate(p, path);
      break;
    default:
      break;
  }
}

/**
 * User-supplied predicate `id`s must be unique across the tree: the planner
 * keys plan nodes, backend calls, and diagnostics on `p.id ?? path`, so a
 * duplicate id conflates two distinct predicates in explain/provenance.
 */
function validatePredicateIds(
  p: Predicate,
  path: string,
  seen: Map<string, { path: string; node: Predicate }>
): void {
  if (typeof p.id === 'string') {
    const first = seen.get(p.id);
    // Sugar expansion (oneOf/xor) legitimately places the SAME predicate
    // object at multiple tree paths — only two DISTINCT nodes sharing an id
    // are a user error.
    if (first !== undefined && first.node !== p) {
      fail(
        diagnostic(
          'invalidQuery',
          `Duplicate predicate id "${p.id}" at ${first.path} and ${path}; ids must be unique across \`where\`.`,
          { queryPath: path, predicateId: p.id }
        )
      );
    }
    if (first === undefined) seen.set(p.id, { path, node: p });
  }
  if (p.kind === 'all' || p.kind === 'any') {
    p.of.forEach((c, i) => validatePredicateIds(c, `${path}.of[${i}]`, seen));
  } else if (p.kind === 'not') {
    validatePredicateIds(p.predicate, `${path}.predicate`, seen);
  }
}

function validateFieldPredicate(
  p: { field: string; op: string; value?: unknown },
  path: string
): void {
  if (p.op === 'exists') {
    if (p.value !== undefined) {
      fail(
        diagnostic('fieldTypeMismatch', '`exists` takes no value.', {
          queryPath: path,
        })
      );
    }
    return;
  }
  if (p.value === undefined) {
    fail(
      diagnostic('fieldTypeMismatch', `Operator "${p.op}" requires a value.`, {
        queryPath: path,
      })
    );
  }
  if (p.op === 'in' && (!Array.isArray(p.value) || p.value.length === 0)) {
    fail(
      diagnostic(
        'fieldTypeMismatch',
        '`in` requires a non-empty array of values.',
        { queryPath: path }
      )
    );
  }
}
