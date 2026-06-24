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
  OqlActiveTarget,
  PlanRoute,
  QuerySource,
} from './types.js';

export interface CapabilityContext {
  sourceKind: QuerySource['kind'];
  target: OqlActiveTarget;
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

/** Local/materialized sources: every supported predicate is evaluated locally. */
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
  // (materialize) or reported as needing a complete universe. (Handled before
  // the files lane so a negated files predicate keeps `negativeUniverseRequired`
  // semantics rather than a generic materialization message.)
  if (inNegation) {
    return negatedOverProvider(ctx, predicate, canMaterialize);
  }

  // `files` target: the GitHub provider can list files *containing a term*
  // (path-level, approximate, via code search), but cannot enumerate files by
  // attribute (field) or run structural/PCRE2. The latter need the local
  // universe. This mirrors executeGithub's files lane so plan and execution
  // agree. Negation already returned above (negativeUniverseRequired/ROUTE).
  if (ctx.target === 'files') {
    const positiveContent =
      predicate.kind === 'text' ||
      (predicate.kind === 'regex' && predicate.dialect !== 'pcre2');
    if (positiveContent) {
      if (canMaterialize) {
        return {
          route: 'ROUTE',
          backend: LOCAL_FIND,
          exact: true,
          reason:
            'files-containing-term routed to materialization for an exact file set',
        };
      }
      return {
        route: 'PUSHDOWN',
        backend: GH_SEARCH,
        exact: false,
        reason:
          'files containing the term listed via provider code search (approximate)',
        diagnostic: {
          code: 'providerSemanticsApproximate',
          message:
            'GitHub lists files containing a term via provider code search; materialize for an exact file set.',
        },
      };
    }
    // Path-like field equality (basename/extension/path "=") maps to provider
    // path qualifiers — the same route the files target and the code field
    // branch use — instead of forcing materialization.
    // When materialization is allowed, prefer it (exact, complete universe),
    // mirroring the positive-content branch above.
    if (
      predicate.kind === 'field' &&
      predicate.op === '=' &&
      (predicate.field === 'path' ||
        predicate.field === 'basename' ||
        predicate.field === 'extension')
    ) {
      if (canMaterialize) {
        return {
          route: 'ROUTE',
          backend: LOCAL_FIND,
          exact: true,
          reason:
            'path/name field equality routed to materialization for an exact file set',
        };
      }
      return {
        route: 'PUSHDOWN',
        backend: GH_SEARCH,
        exact: true,
        reason: 'path/name field equality listed via provider path search',
      };
    }
    if (canMaterialize) {
      return {
        route: 'ROUTE',
        backend: LOCAL_FIND,
        exact: true,
        reason: `${describeLeaf(predicate)} over a file listing routed to materialization`,
      };
    }
    return {
      route: 'UNSUPPORTED',
      backend: LOCAL_FIND,
      exact: false,
      reason: `GitHub cannot enumerate files by ${describeLeaf(predicate)} without materialization`,
      diagnostic: {
        code: 'requiresMaterialization',
        message: `target:"files" over GitHub cannot enumerate by ${describeLeaf(predicate)} without materialization (set materialize.mode "auto"/"required" with scope.path).`,
      },
    };
  }

  switch (predicate.kind) {
    case 'text': {
      if (ctx.materialize?.mode === 'required') {
        return {
          route: 'ROUTE',
          backend: LOCAL_SEARCH,
          exact: true,
          reason:
            'literal text routed to materialization because materialize.mode is required',
        };
      }
      // GitHub code search is a case-insensitive substring match: it cannot
      // honor case:sensitive or wholeWord (the compiled flags are dropped by
      // transformers/github/code.ts because ghSearchCode has no equivalent).
      // So such a predicate is approximate, never proof — route to bounded
      // materialization when allowed, otherwise push down but mark the decision
      // non-exact so the plan does not claim proof.
      const providerCannotHonor =
        predicate.case === 'sensitive' || predicate.wholeWord === true;
      if (providerCannotHonor) {
        if (canMaterialize) {
          return {
            route: 'ROUTE',
            backend: LOCAL_SEARCH,
            exact: true,
            reason:
              'case-sensitive / whole-word text routed to materialization for exact proof',
          };
        }
        return {
          route: 'PUSHDOWN',
          backend: GH_SEARCH,
          exact: false,
          reason:
            'GitHub code search cannot honor case:sensitive / wholeWord (case-insensitive substring); approximate',
          diagnostic: {
            code: 'providerSemanticsApproximate',
            message:
              'GitHub code search is a case-insensitive substring match and cannot honor case:sensitive or wholeWord; materialize for exact proof.',
          },
        };
      }
      return {
        route: 'PUSHDOWN',
        backend: GH_SEARCH,
        exact: true,
        reason: 'literal text pushed to GitHub code search',
      };
    }
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
