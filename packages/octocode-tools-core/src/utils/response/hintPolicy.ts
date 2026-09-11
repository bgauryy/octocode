import {
  AST_SEARCH_TOOL_NAME,
  GITHUB_GET_HISTORY_ITEM_TOOL_NAME,
  GITHUB_SEARCH_HISTORY_TOOL_NAME,
  GITHUB_SEARCH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
  LSP_SEARCH_TOOL_NAME,
  STATIC_TOOL_NAMES,
} from '@octocodeai/octocode-core/schema';

const MAX_GUIDANCE_CHARS = 120;

// These calls suggest new research; they do not continue a bounded result.
// Unknown next keys are preserved so adding a continuation cannot lose data.
const ADVISORY_CALLS = new Set([
  'fetch',
  'getLines',
  'readSite',
  'viewDeeper',
  'viewStructure',
  'viewTree',
  'searchCode',
  'searchRepositoryCode',
  'cloneRepo',
  'cloneForSemantics',
  'lspDefinition',
  'lspReferences',
  'readIssue',
  'prDetail',
]);

// Visit response metadata only. Content, matches, bodies, patches and executable
// queries are evidence and may themselves contain fields named hints or next.
const METADATA_CONTAINERS = new Set([
  'data',
  'meta',
  'diagnostics',
  'error',
  'files',
  'directories',
  'results',
  'packages',
  'entries',
  'items',
]);

function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function concise(value: string): string {
  let text = value.replace(/\s+/g, ' ').trim();
  if (text && !/[.!?…]$/.test(text)) text += '.';
  if (text.length <= MAX_GUIDANCE_CHARS) return text;
  const prefix = text.slice(0, MAX_GUIDANCE_CHARS - 1);
  const boundary = prefix.lastIndexOf(' ');
  return `${prefix.slice(0, boundary > 60 ? boundary : MAX_GUIDANCE_CHARS - 1)}…`;
}

function hasExecutableCall(value: unknown): boolean {
  const call = record(value);
  return typeof call?.tool === 'string' && record(call.query) !== undefined;
}

function hasRecovery(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(hasRecovery);
  const node = record(value);
  if (!node) return false;
  if (
    Array.isArray(node.hints) &&
    node.hints.some(hint => typeof hint === 'string' && hint.trim())
  )
    return true;
  if (Object.values(record(node.next) ?? {}).some(hasExecutableCall))
    return true;
  for (const [key, child] of Object.entries(node)) {
    if (METADATA_CONTAINERS.has(key) && hasRecovery(child)) return true;
    if (
      key === 'repositories' &&
      Object.values(record(child) ?? {}).some(hasRecovery)
    )
      return true;
  }
  return false;
}

const ACTIONABLE_ERROR =
  /\b(?:broaden|check|choose|correct|disable|enable|pass|provide|refresh|remove|retry|run|select|set|specify|supply|try|use|verify|wait)\b/i;

function hasActionableError(value: unknown): boolean {
  const node = record(value);
  if (!node) return false;
  const error = node.error;
  if (typeof error === 'string' && ACTIONABLE_ERROR.test(error)) return true;
  const nested = record(error)?.error;
  return typeof nested === 'string' && ACTIONABLE_ERROR.test(nested);
}

function fallbackHint(
  toolName: string | undefined,
  queryValue: unknown
): string | undefined {
  const query = record(queryValue) ?? {};
  switch (toolName) {
    case GITHUB_SEARCH_TOOL_NAME:
      return query.operation === 'tree'
        ? 'Verify owner/repo/branch, or broaden path/depth.'
        : 'Broaden keywords or remove filters.';
    case STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT:
      return 'Verify owner/repo/branch/path, or remove matchString.';
    case GITHUB_SEARCH_HISTORY_TOOL_NAME:
      return 'Broaden keywords or remove history filters.';
    case GITHUB_GET_HISTORY_ITEM_TOOL_NAME:
      return 'Verify owner/repo and the number, ref, or compare refs.';
    case STATIC_TOOL_NAMES.PACKAGE_SEARCH:
      return 'Check packageName, or broaden keywords.';
    case STATIC_TOOL_NAMES.GITHUB_CLONE_REPO:
      return 'Verify owner/repo/branch and sparsePath.';
    case LOCAL_SEARCH_TOOL_NAME:
      return 'Broaden searchText, path, or filters.';
    case AST_SEARCH_TOOL_NAME:
      if (query.operation === 'files' || query.treeKind === 'filesystem')
        return 'Broaden path or file filters.';
      if (query.operation === 'topology')
        return 'Inspect diagnostics, then broaden the graph scope if needed.';
      return 'Broaden the syntax/name query, path, or filters.';
    case STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT:
      return 'Verify path/range, or remove matchString.';
    case LSP_SEARCH_TOOL_NAME:
      return 'Refresh uri/symbolName/lineHint, or broaden workspaceRoot.';
    default:
      return undefined;
  }
}

export interface HintPolicyContext {
  toolName?: string;
  queries?: readonly unknown[];
}

function addFallbackHint(
  value: unknown,
  position: number,
  context: HintPolicyContext
): void {
  const row = record(value);
  if (!row || (row.status !== 'empty' && row.status !== 'error')) return;
  if (hasRecovery(row)) return;
  if (row.status === 'error' && hasActionableError(row.data)) return;
  const data = record(row.data);
  if (!data) return;
  const queryIndex = typeof row.index === 'number' ? row.index : position;
  const hint = fallbackHint(context.toolName, context.queries?.[queryIndex]);
  if (hint) data.hints = [hint];
}

function shapeNext(value: unknown, recovery: boolean): unknown {
  const next = record(value);
  if (!next) return value;
  return Object.fromEntries(
    Object.entries(next).flatMap(([key, value]) => {
      const call = record(value);
      if (!call || typeof call.tool !== 'string' || !record(call.query)) {
        return [[key, value]];
      }
      if (!recovery && ADVISORY_CALLS.has(key.split(':')[0]!)) return [];
      const { why, ...rest } = call;
      return [
        [
          key,
          recovery && typeof why === 'string'
            ? { ...rest, why: concise(why) }
            : rest,
        ],
      ];
    })
  );
}

function visit(value: unknown, recovery: boolean, seen: Set<string>): void {
  if (Array.isArray(value)) {
    for (const child of value) visit(child, recovery, seen);
    return;
  }
  const node = record(value);
  if (!node) return;
  const needsHelp =
    node.status === 'error' || node.status === 'empty' || recovery;
  if ('next' in node) node.next = shapeNext(node.next, needsHelp);
  for (const [key, child] of Object.entries(node)) {
    if (METADATA_CONTAINERS.has(key)) visit(child, needsHelp, seen);
    if (key === 'repositories') {
      for (const repo of Object.values(record(child) ?? {}))
        visit(repo, needsHelp, seen);
    }
  }
  if ('hints' in node) {
    const hints: string[] = [];
    if (needsHelp && Array.isArray(node.hints)) {
      const candidates = node.hints
        .filter((hint): hint is string => typeof hint === 'string')
        .map(concise)
        .filter(Boolean)
        .sort(
          (left, right) =>
            Number(ACTIONABLE_ERROR.test(right)) -
            Number(ACTIONABLE_ERROR.test(left))
        );
      for (const short of candidates) {
        if (!short || seen.has(short) || seen.size >= 1) continue;
        seen.add(short);
        hints.push(short);
      }
    }
    if (hints.length) node.hints = hints;
    else delete node.hints;
  }
}

/** Apply once after tool finalization, before rendering either public channel. */
export function applyHintPolicy(
  rows: unknown[],
  context: HintPolicyContext = {}
): void {
  rows.forEach((row, index) => {
    addFallbackHint(row, index, context);
    visit(row, false, new Set());
  });
}
