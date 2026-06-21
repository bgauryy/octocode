/**
 * Human/agent-readable OQL schema description, served by
 * `octocode search --scheme`. This is the V1 contract surface; the canonical
 * language reference lives in docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md.
 */
import { DEFAULTS } from './defaults.js';
import { ACTIVE_TARGETS, RESERVED_TARGETS } from './types.js';

export const OQL_SCHEMA_DOC = {
  schema: 'oql/v1',
  description:
    'Use octocode search for bounded research over local paths and GitHub scopes: search code matches, file lists, directory trees, or exact/minified content; set from, scope, and where.kind; keep output small with view/select/controls; materialize only for bounded local proof; use --explain and follow next.* continuations when routing or paging is uncertain.',
  activeTargets: ACTIVE_TARGETS,
  reservedTargets: RESERVED_TARGETS,
  query: {
    schema: '"oql/v1" (inserted by normalization)',
    target: 'code | content | structure | files',
    from: '{ kind:"local", path } | { kind:"github", repo?, owner?, ref? } | { kind:"materialized", localPath, source? }',
    scope:
      '{ path?, language?, include?, exclude?, excludeDir?, hidden?, noIgnore?, maxDepth? }',
    where:
      'discriminated predicate: text | regex | structural | field | all | any | not',
    materialize:
      '{ mode:"never"|"auto"|"required", strategy?, allowFullRepo?, forceRefresh? }',
    fetch: '{ content?: {...}, tree?: {...} }',
    select: 'string[] projection of result/continuation fields',
    view: 'discovery | paginated | detailed',
    controls: '{ search?: {...}, budget?: {...} }',
    limit: 'number',
    page: 'number',
    itemsPerPage: 'number',
    explain: 'boolean',
  },
  predicates: {
    text: '{ kind:"text", value, case?, wholeWord? }',
    regex:
      '{ kind:"regex", value, dialect?:"rust"|"pcre2"|"provider", case?, wholeWord?, multiline?, dotAll? }',
    structural:
      '{ kind:"structural", lang, pattern? | rule? } (exactly one of pattern/rule)',
    field:
      '{ kind:"field", field:"path"|"basename"|"extension"|"size"|"modified"|"entryType", op, value? }',
    boolean:
      '{ kind:"all"|"any", of: Predicate[] } | { kind:"not", predicate }',
  },
  batch: {
    queries: 'OqlQuery[] (1-5)',
    combine: 'independent | merge',
  },
  defaults: DEFAULTS,
} as const;

export function oqlSchemaText(): string {
  return JSON.stringify(OQL_SCHEMA_DOC, null, 2);
}
