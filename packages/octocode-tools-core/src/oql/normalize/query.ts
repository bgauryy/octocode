/* ------------------------------- query ------------------------------------ */
import { OqlInputQuerySchema, OqlQuerySchema } from '../schema.js';
import { diagnostic } from '../diagnostics.js';
import { validateTargetParams } from '../targetParams.js';
import {
  ACTIVE_TARGETS,
  RESERVED_TARGETS,
  type OqlInputQuery,
  type OqlQuery,
} from '../types.js';
import { fail, formatZodError } from './shared.js';
import { normalizeSource } from './source.js';
import { normalizeScope } from './scope.js';
import { normalizeWhere } from './where.js';
import { normalizeMaterialize } from './materialize.js';
import { normalizeParams } from './params.js';

/** Canonical + sugar keys consumed by the normalizer. */
const KNOWN_QUERY_KEYS = new Set<string>([
  // canonical
  'schema',
  'id',
  'target',
  'from',
  'scope',
  'where',
  'materialize',
  'fetch',
  'select',
  'view',
  'controls',
  'limit',
  'page',
  'itemsPerPage',
  'params',
  'explain',
  // sugar
  'repo',
  'owner',
  'ref',
  'path',
  'text',
  'regex',
  'pattern',
  'rule',
  'lang',
  'and',
  'or',
  'xor',
  'noneOf',
  'oneOf',
  'invert',
  'filesOnly',
  'filesWithoutMatch',
  // base-query meta auto-filled by interfaces; ignored by OQL
  'mainResearchGoal',
  'researchGoal',
  'reasoning',
  'verbose',
]);

export function normalizeQuery(input: OqlInputQuery): OqlQuery {
  const parsed = OqlInputQuerySchema.safeParse(input);
  if (!parsed.success) {
    fail(diagnostic('invalidQuery', formatZodError(parsed.error)));
  }
  const raw = {
    ...(parsed.data as Record<string, unknown>),
  } as OqlInputQuery;

  // 1. resolve target. `filesWithoutMatch` sugar forces "files"; otherwise use
  // the explicit target or infer it from the rest of the query.
  const target = raw.filesWithoutMatch
    ? 'files'
    : (raw.target ?? inferTarget(raw));
  if (target === undefined) {
    fail(
      diagnostic(
        'invalidQuery',
        `Could not determine \`target\`; specify one of: ${ACTIVE_TARGETS.join(', ')}.`,
        { queryPath: 'target' }
      )
    );
  }
  if ((RESERVED_TARGETS as readonly string[]).includes(target)) {
    fail(
      diagnostic(
        'unsupportedTarget',
        `Target "${target}" is reserved until proof/dry-run support exists.`,
        {
          queryPath: 'target',
          repair: {
            message: `Use an active target: ${ACTIVE_TARGETS.join(', ')}.`,
          },
        }
      )
    );
  }
  if (!(ACTIVE_TARGETS as readonly string[]).includes(target)) {
    fail(diagnostic('unknownField', `Unknown target "${target}".`));
  }

  // 2. reject unknown top-level keys
  for (const key of Object.keys(raw)) {
    if (!KNOWN_QUERY_KEYS.has(key)) {
      fail(
        diagnostic(
          'unknownField',
          `Unknown field "${key}" is not part of OQL.`,
          {
            queryPath: key,
          }
        )
      );
    }
  }

  const select =
    raw.filesOnly === true
      ? Array.isArray(raw.select)
        ? raw.select
        : ['path', 'next.fetch']
      : raw.select;
  const view: OqlQuery['view'] =
    raw.filesOnly === true ? 'discovery' : (raw.view ?? 'paginated');

  const from = normalizeSource(raw, target as OqlQuery['target']);
  const scope = normalizeScope(raw, from);
  const where = normalizeWhere(raw, target as OqlQuery['target']);
  const materialize = normalizeMaterialize(
    raw,
    from,
    where,
    target as OqlQuery['target']
  );
  const fetch = normalizeFetch(raw);
  const params = normalizeParams(raw, target as OqlQuery['target']);

  const canonical: OqlQuery = {
    schema: 'oql',
    ...(raw.id ? { id: raw.id } : {}),
    target: target as OqlQuery['target'],
    ...(from ? { from } : {}),
    ...(params ? { params } : {}),
    ...(scope ? { scope } : {}),
    ...(where ? { where } : {}),
    ...(materialize ? { materialize } : {}),
    ...(fetch ? { fetch } : {}),
    ...(select ? { select } : {}),
    view,
    ...(raw.controls ? { controls: raw.controls } : {}),
    ...(raw.limit !== undefined ? { limit: raw.limit } : {}),
    ...(raw.page !== undefined ? { page: raw.page } : {}),
    ...(raw.itemsPerPage !== undefined
      ? { itemsPerPage: raw.itemsPerPage }
      : {}),
    ...(raw.explain !== undefined ? { explain: raw.explain } : {}),
  };

  // `code` requires a real predicate.
  if (canonical.target === 'code' && !canonical.where) {
    fail(
      diagnostic(
        'invalidQuery',
        'target:"code" requires a `where` predicate (text/regex/structural). `where` omission is not a wildcard.',
        { queryPath: 'where' }
      )
    );
  }

  // `content`/`structure` do not evaluate `where` (the execution layer would
  // silently drop it). Reject rather than drop — no predicate may disappear.
  if (
    (canonical.target === 'content' || canonical.target === 'structure') &&
    canonical.where
  ) {
    fail(
      diagnostic(
        'invalidQuery',
        `target:"${canonical.target}" does not use \`where\`. Use fetch.content.match for content anchors, or target:"code"/"files" for predicates.`,
        { queryPath: 'where' }
      )
    );
  }

  // `content`/`structure` reads of a specific tree require a concrete GitHub
  // repository (`owner/name`). Provider-wide or owner-only GitHub sources are
  // valid only for provider-search targets (code/repositories), not for fetching
  // a specific file or directory tree (contract §source-and-scope).
  if (
    (canonical.target === 'content' || canonical.target === 'structure') &&
    canonical.from?.kind === 'github' &&
    !(canonical.from.repo && canonical.from.repo.includes('/'))
  ) {
    fail(
      diagnostic(
        'invalidQuery',
        `target:"${canonical.target}" over GitHub requires a concrete repository ("owner/name"); a provider-wide or owner-only source cannot read a specific tree.`,
        {
          queryPath: 'from',
          repair: {
            message:
              'Set from:{kind:"github",repo:"owner/name"} (and scope.path for a subtree).',
          },
        }
      )
    );
  }

  // target:"materialize" is a clone checkpoint, not a search: it takes no
  // `where`, and needs a materializable source (GitHub repo, or an already
  // materialized path to echo).
  if (canonical.target === 'materialize') {
    if (canonical.where) {
      fail(
        diagnostic(
          'invalidQuery',
          'target:"materialize" does not use `where`; it clones/caches a corpus and returns a stable local checkpoint. Run a search against the returned localPath instead.',
          { queryPath: 'where' }
        )
      );
    }
    if (
      canonical.from?.kind !== 'github' &&
      canonical.from?.kind !== 'materialized'
    ) {
      fail(
        diagnostic(
          'invalidQuery',
          'target:"materialize" needs from:{kind:"github",repo:"owner/name"} (and scope.path to bound the subtree) or an already-materialized `from`.',
          {
            queryPath: 'from',
            repair: {
              message:
                'Set from:{kind:"github",repo:"owner/name"} with scope.path.',
            },
          }
        )
      );
    }
  }

  // Typed params check: catch type mistakes on known params fields early
  // (the backing tool remains the exhaustive validator for the rest).
  if (canonical.params !== undefined) {
    const paramsError = validateTargetParams(
      canonical.target,
      canonical.params
    );
    if (paramsError) {
      fail(diagnostic('invalidQuery', paramsError, { queryPath: 'params' }));
    }
  }

  // Validate the final canonical object against the strict schema.
  const check = OqlQuerySchema.safeParse(canonical);
  if (!check.success) {
    fail(diagnostic('invalidQuery', formatZodError(check.error)));
  }
  return check.data as OqlQuery;
}

/**
 * Infer the result target from sugar when not explicit:
 *  - any match sugar / canonical `where` -> "code"
 *  - fetch.content -> "content"
 *  - fetch.tree -> "structure"
 */
function inferTarget(raw: OqlInputQuery): OqlQuery['target'] | undefined {
  const hasMatch =
    raw.where !== undefined ||
    typeof raw.text === 'string' ||
    typeof raw.regex === 'string' ||
    typeof raw.pattern === 'string' ||
    raw.rule !== undefined ||
    Array.isArray(raw.and) ||
    Array.isArray(raw.or) ||
    Array.isArray(raw.xor) ||
    Array.isArray(raw.noneOf) ||
    Array.isArray(raw.oneOf);
  if (hasMatch) return 'code';
  if (raw.fetch?.content) return 'content';
  if (raw.fetch?.tree) return 'structure';
  return undefined;
}

/* ------------------------------- fetch ---------------------------------- */

function normalizeFetch(raw: OqlInputQuery): OqlQuery['fetch'] | undefined {
  return raw.fetch ? { ...raw.fetch } : undefined;
}
