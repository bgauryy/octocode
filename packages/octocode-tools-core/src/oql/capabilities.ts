/**
 * OQL capability registry — makes backend limits explicit and testable.
 *
 * Given the source kind, target, materialization stance, and a single leaf
 * predicate, decide how that predicate can be evaluated:
 *   PUSHDOWN   backend evaluates it directly (exact or approximate)
 *   RESIDUAL   fetch bounded candidates and filter locally
 *   ROUTE      move to another lane (materialize -> local)
 *   UNSUPPORTED cannot be evaluated; emit a diagnostic
 *
 * This is intentionally conservative: anything that cannot be proven exactly is
 * flagged so the planner never reports `proof` it cannot back.
 */
import type {
  DiagnosticCode,
  LeafPredicate,
  MaterializePolicy,
  OqlActiveTargetV1,
  PlanRoute,
  QuerySource,
} from './types.js';

export interface CapabilityContext {
  sourceKind: QuerySource['kind'];
  target: OqlActiveTargetV1;
  materialize: MaterializePolicy | undefined;
}

export interface CapabilityDecision {
  route: PlanRoute;
  backend: string;
  /** Whether the backend evaluates the predicate exactly (proof-grade). */
  exact: boolean;
  reason: string;
  diagnostic?: { code: DiagnosticCode; message: string };
}

const LOCAL_SEARCH = 'localSearchCode';
const LOCAL_FIND = 'localFindFiles';
const GH_SEARCH = 'ghSearchCode';

function materializeAllowed(m: MaterializePolicy | undefined): boolean {
  return m?.mode === 'auto' || m?.mode === 'required';
}

/** Local/materialized sources: every V1 predicate is evaluated locally. */
function routeLocal(
  ctx: CapabilityContext,
  predicate: LeafPredicate
): CapabilityDecision {
  const backend = ctx.target === 'files' ? LOCAL_FIND : LOCAL_SEARCH;
  switch (predicate.kind) {
    case 'text':
    case 'regex':
    case 'structural':
      return {
        route: 'PUSHDOWN',
        backend: LOCAL_SEARCH,
        exact: true,
        reason: `${predicate.kind} evaluated locally by ${LOCAL_SEARCH}`,
      };
    case 'field':
      return {
        route: 'PUSHDOWN',
        backend,
        exact: true,
        reason: `field predicate evaluated locally by ${backend}`,
      };
  }
}

/** GitHub provider source. */
function routeGithub(
  ctx: CapabilityContext,
  predicate: LeafPredicate,
  inNegation: boolean
): CapabilityDecision {
  const canMaterialize = materializeAllowed(ctx.materialize);

  // Under negation, the provider can never prove *absence*: provider zero-
  // results are not proof unless the candidate universe is complete. Any
  // predicate evaluated inside a `not` must therefore be proven locally
  // (materialize) or reported as needing a complete universe.
  if (inNegation) {
    return negatedOverProvider(ctx, predicate, canMaterialize);
  }

  switch (predicate.kind) {
    case 'text':
      return {
        route: 'PUSHDOWN',
        backend: GH_SEARCH,
        exact: true,
        reason: 'literal text pushed to GitHub code search',
      };
    case 'regex': {
      if (predicate.dialect === 'pcre2') {
        return localOnlyOverProvider(ctx, 'PCRE2 regex', canMaterialize);
      }
      // rust/provider regex: provider search is approximate, not proof-grade.
      if (canMaterialize) {
        return {
          route: 'ROUTE',
          backend: LOCAL_SEARCH,
          exact: true,
          reason:
            'regex routed to bounded materialization for exact local proof',
        };
      }
      return {
        route: 'PUSHDOWN',
        backend: GH_SEARCH,
        exact: false,
        reason: 'regex pushed to GitHub search (provider regex is approximate)',
        diagnostic: {
          code: 'providerSemanticsApproximate',
          message:
            'GitHub regex search is approximate; materialize for exact regex proof.',
        },
      };
    }
    case 'structural':
      return localOnlyOverProvider(ctx, 'structural AST', canMaterialize);
    case 'field': {
      if (
        predicate.field === 'path' ||
        predicate.field === 'basename' ||
        predicate.field === 'extension'
      ) {
        if (predicate.op === 'glob' || predicate.op === 'regex') {
          if (canMaterialize) {
            return {
              route: 'ROUTE',
              backend: LOCAL_FIND,
              exact: true,
              reason: 'path glob/regex routed to materialization for proof',
            };
          }
          return {
            route: 'PUSHDOWN',
            backend: GH_SEARCH,
            exact: false,
            reason: 'provider path filter is prefix-only / approximate',
            diagnostic: {
              code: 'providerSemanticsApproximate',
              message:
                'GitHub path qualifiers are prefix filters; materialize to prove glob/regex.',
            },
          };
        }
        return {
          route: 'PUSHDOWN',
          backend: GH_SEARCH,
          exact: true,
          reason: 'path/name predicate pushed to provider',
        };
      }
      // size/modified/entryType: provider cannot prove these.
      return localOnlyOverProvider(
        ctx,
        `field "${predicate.field}"`,
        canMaterialize
      );
    }
  }
}

/** Describe a leaf predicate for diagnostic/reason text. */
function describeLeaf(predicate: LeafPredicate): string {
  switch (predicate.kind) {
    case 'text':
      return 'text match';
    case 'regex':
      return predicate.dialect === 'pcre2' ? 'PCRE2 regex' : 'regex match';
    case 'structural':
      return 'structural AST match';
    case 'field':
      return `field "${predicate.field}"`;
  }
}

/**
 * A predicate evaluated inside a `not` over a GitHub provider source. The
 * provider cannot enumerate the complete candidate universe, so a negation can
 * only be proven by materializing bounded code and running the local tool; with
 * `materialize.mode:"never"` it is `negativeUniverseRequired`.
 */
function negatedOverProvider(
  ctx: CapabilityContext,
  predicate: LeafPredicate,
  canMaterialize: boolean
): CapabilityDecision {
  const what = describeLeaf(predicate);
  if (canMaterialize) {
    return {
      route: 'ROUTE',
      backend: ctx.target === 'files' ? LOCAL_FIND : LOCAL_SEARCH,
      exact: true,
      reason: `negated ${what} needs a complete universe; routed to bounded materialization for local proof`,
    };
  }
  return {
    route: 'UNSUPPORTED',
    backend: GH_SEARCH,
    exact: false,
    reason: `negated ${what} cannot be proven by the GitHub provider (no complete universe)`,
    diagnostic: {
      code: 'negativeUniverseRequired',
      message:
        'Negation over a GitHub provider source needs a complete candidate universe; materialize to prove absence.',
    },
  };
}

function localOnlyOverProvider(
  ctx: CapabilityContext,
  what: string,
  canMaterialize: boolean
): CapabilityDecision {
  if (canMaterialize) {
    return {
      route: 'ROUTE',
      backend: LOCAL_SEARCH,
      exact: true,
      reason: `${what} requires local proof; routed to bounded materialization`,
    };
  }
  const mode = ctx.materialize?.mode;
  return {
    route: 'UNSUPPORTED',
    backend: GH_SEARCH,
    exact: false,
    reason: `${what} cannot be evaluated by the GitHub provider`,
    diagnostic:
      mode === 'never'
        ? {
            code: 'materializationNotAllowed',
            message: `${what} needs local proof but materialize.mode is "never".`,
          }
        : {
            code: 'requiresMaterialization',
            message: `${what} needs bounded materialization (set materialize.mode "auto" or "required").`,
          },
  };
}

export function routeLeafPredicate(
  ctx: CapabilityContext,
  predicate: LeafPredicate,
  inNegation = false
): CapabilityDecision {
  if (ctx.sourceKind === 'github') {
    return routeGithub(ctx, predicate, inNegation);
  }
  // local + materialized are both proven locally (the complete universe is
  // available there, so negation is exact without materialization).
  return routeLocal(ctx, predicate);
}
