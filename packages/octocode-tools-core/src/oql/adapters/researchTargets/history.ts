/**
 * `target:"pullRequests"` and `target:"commits"` adapter (both route through
 * ghHistoryResearch): forwards OQL paging, then applies the P4 client-side
 * `matchString` content filter (never a backing search-index claim) with
 * honest partial/zero-match diagnostics.
 */
import { runDirect } from '../runner.js';
import { diagnostic } from '../../diagnostics.js';
import type { AdapterResult } from '../local.js';
import { finishRecords } from './pagination.js';
import { params, splitRepo, withOqlPaging } from './shared.js';
import type { OqlQuery, OqlRecordResultRow } from '../../types.js';

export async function executeHistory(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(query.from);
  const commits = query.target === 'commits';

  // P4: `matchString` is an OQL-layer *content* filter applied to fetched
  // bodies — never a backing search-index claim. Strip it (and matchScope) from
  // the params forwarded to ghHistoryResearch for BOTH lanes so the tool is not
  // asked to interpret it as a query field, then apply it client-side with
  // honest partial/zero-match diagnostics. (Commits previously forwarded
  // matchString raw and never filtered — a silent drop if the backend ignored
  // it; PRs and commits now share the same content-filter discipline.)
  const pr = !commits ? pullRequestMatch(query) : undefined;
  const commitNeedle = commits ? commitMatchNeedle(query) : undefined;
  const forwarded = withOqlPaging(query, commits ? 'perPage' : 'limit');
  if (pr || commitNeedle) {
    delete forwarded.matchString;
    delete forwarded.matchScope;
  }

  const result = await runDirect('ghHistoryResearch', {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    ...(commits ? { type: 'commits' } : {}),
    ...forwarded,
  });
  const mapped = finishRecords(
    result,
    commits ? 'commit' : 'pullRequest',
    'ghHistoryResearch',
    query.from ?? { kind: 'github' }
  );
  if (pr) return filterPullRequestsByMatch(mapped, pr);
  if (commitNeedle) return filterCommitsByMatch(mapped, commitNeedle);
  return mapped;
}

/** Read the validated commit content-match needle, if present. */
function commitMatchNeedle(query: OqlQuery): string | undefined {
  const p = params(query);
  return typeof p.matchString === 'string' && p.matchString.length > 0
    ? p.matchString
    : undefined;
}

/**
 * Keep only commit records whose message contains `needle` (case-insensitive
 * substring), spotlight where it matched, and surface honest diagnostics — a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 * Mirrors {@link filterPullRequestsByMatch}; commit text is the commit message.
 */
export function filterCommitsByMatch(
  result: AdapterResult,
  needle: string
): AdapterResult {
  const needleLower = needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const messageVal = (data as Record<string, unknown>).message;
    const haystack = typeof messageVal === 'string' ? messageVal : '';
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: needle,
      scope: 'message',
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No commit message matched "${needle}" (content filter over ${total} fetched commit(s); not a search-index query). Broaden the fetch (branch/perPage/page).`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched commit(s) matching "${needle}" in message. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}

export interface PullRequestMatch {
  needle: string;
  scope: 'body' | 'title' | 'comments' | 'reviews' | 'all';
}

/** Read the validated PR content-match params, if present. */
function pullRequestMatch(query: OqlQuery): PullRequestMatch | undefined {
  const p = params(query);
  const needle = typeof p.matchString === 'string' ? p.matchString : undefined;
  if (!needle) return undefined;
  const scope =
    p.matchScope === 'title' ||
    p.matchScope === 'comments' ||
    p.matchScope === 'reviews' ||
    p.matchScope === 'all'
      ? p.matchScope
      : 'body';
  return { needle, scope };
}

/** Collect the searchable text for a PR record under the requested scope. */
function pullRequestScopeText(
  data: Record<string, unknown>,
  scope: PullRequestMatch['scope']
): string {
  const parts: string[] = [];
  const push = (v: unknown) => {
    if (typeof v === 'string' && v.length > 0) parts.push(v);
  };
  const bodies = (key: string) => {
    const list = data[key];
    if (Array.isArray(list)) {
      for (const c of list) {
        if (c && typeof c === 'object') push((c as { body?: unknown }).body);
      }
    }
  };
  if (scope === 'body' || scope === 'all') push(data.body);
  if (scope === 'title' || scope === 'all') push(data.title);
  if (scope === 'comments' || scope === 'all') bodies('comments');
  if (scope === 'reviews' || scope === 'all') bodies('reviews');
  return parts.join('\n');
}

/**
 * Keep only PR records whose scope text contains `matchString` (case-insensitive
 * substring), spotlight where each matched, and surface honest diagnostics: a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 */
export function filterPullRequestsByMatch(
  result: AdapterResult,
  match: PullRequestMatch
): AdapterResult {
  const needleLower = match.needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const haystack = pullRequestScopeText(data, match.scope);
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    // Additive spotlight: a bounded window around the first hit (full body/
    // comment text is left intact on the record).
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + match.needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: match.needle,
      scope: match.scope,
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No pull request ${match.scope} matched "${match.needle}" (content filter over ${total} fetched PR(s); not a search-index query). Broaden the fetch (state/keywordsToSearch/page) or the match scope.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched PR(s) matching "${match.needle}" in ${match.scope}. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}
