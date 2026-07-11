/**
 * GitHub provider adapter — status/error classification. Turns a raw
 * `CallToolResult` from a GitHub tool runner into OQL diagnostics: provider
 * failures (rate limit / auth / invalid query), zero-match status, and the
 * "empty result is not proof" guard for provider reads/searches.
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { diagnostic } from '../../diagnostics.js';
import type { OqlDiagnostic } from '../../types.js';
import { extractData, extractStatus } from './shared.js';

/**
 * GitHub provider zero-results are NOT silent proof — code search can be
 * unindexed/deprecated and repo names redirect. Emit a blocking diagnostic so
 * an empty provider read/search cannot be presented as complete proof.
 */
function emptyProviderDiag(rowCount: number, backend: string): OqlDiagnostic[] {
  if (rowCount > 0) return [];
  return [
    diagnostic(
      'providerUnindexed',
      `${backend} returned no results — GitHub may not index this repo/branch (or the name redirected). Do not treat this as absence: verify with \`search owner/repo[/path] --tree\`, then use bounded local proof via \`search <term> <path> --repo owner/repo --materialize required\`, \`clone owner/repo[/path]\`, or \`cache fetch owner/repo [path] --depth file|tree|clone\`.`,
      { backend, severity: 'warning', blocksAnswer: true }
    ),
  ];
}

type ProviderErrorInfo = {
  message: string;
  status?: number;
  retryAfterSeconds?: number;
  rateLimitRemaining?: number;
};

function finiteNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

/**
 * Read a provider failure from BOTH result shapes:
 * - default bulk path: `results[0].status === 'error'` with `data.error`
 *   carrying the structured provider error (or a plain string);
 * - finalized tools (ghSearchCode/ghGetFileContent): errored results are
 *   stripped into top-level `errors[]`. OQL sends exactly one query per
 *   runDirect call, so `errors[0]` is that query's failure — without this
 *   read, a 403/429 used to fire no status diagnostic at all and surfaced
 *   as the misleading providerUnindexed.
 */
function providerErrorInfo(
  result: CallToolResult
): ProviderErrorInfo | undefined {
  if (extractStatus(result) === 'error') {
    const err = extractData<{ error?: unknown }>(result)?.error;
    if (typeof err === 'string' && err) return { message: err };
    if (err && typeof err === 'object') {
      const o = err as Record<string, unknown>;
      return {
        message:
          typeof o.error === 'string' && o.error
            ? o.error
            : 'GitHub backend error',
        status: finiteNumber(o.status),
        retryAfterSeconds: finiteNumber(o.retryAfter),
        rateLimitRemaining: finiteNumber(o.rateLimitRemaining),
      };
    }
    return { message: 'GitHub backend error' };
  }
  const sc = result.structuredContent as
    | {
        errors?: Array<{
          error?: unknown;
          status?: unknown;
          retryAfterSeconds?: unknown;
          rateLimitRemaining?: unknown;
        }>;
      }
    | undefined;
  const e = sc?.errors?.[0];
  if (!e) return undefined;
  return {
    message:
      typeof e.error === 'string' && e.error ? e.error : 'GitHub backend error',
    status: finiteNumber(e.status),
    retryAfterSeconds: finiteNumber(e.retryAfterSeconds),
    rateLimitRemaining: finiteNumber(e.rateLimitRemaining),
  };
}

function classifyProviderError(
  info: ProviderErrorInfo,
  backend: string
): OqlDiagnostic {
  const rateLimitLike = /rate limit|secondary rate/i.test(info.message);
  const authLike =
    /bad credentials|requires authentication|saml|not accessible by/i.test(
      info.message
    );
  if (
    info.status === 429 ||
    (info.status === 403 &&
      (info.rateLimitRemaining === 0 ||
        info.retryAfterSeconds !== undefined ||
        rateLimitLike)) ||
    (info.status === undefined && rateLimitLike)
  ) {
    const wait =
      info.retryAfterSeconds !== undefined
        ? ` Retry after ~${info.retryAfterSeconds}s.`
        : '';
    // Transient → warning severity, but still blocks proof via BLOCKING_CODES:
    // a rate-limited call evaluated nothing.
    return diagnostic('rateLimited', `${info.message}${wait}`, {
      backend,
      severity: 'warning',
      repair: {
        message:
          'Wait for the rate-limit window to reset and re-run the same query, or authenticate (OCTOCODE_TOKEN/GH_TOKEN/GITHUB_TOKEN) to raise limits.',
      },
    });
  }
  if (
    info.status === 401 ||
    ((info.status === 403 || info.status === undefined) && authLike)
  ) {
    return diagnostic('authRequired', info.message, {
      backend,
      repair: {
        message:
          'Provide a valid token (OCTOCODE_TOKEN/GH_TOKEN/GITHUB_TOKEN) with access to this repo, then re-run the same query.',
      },
    });
  }
  return diagnostic('invalidQuery', info.message, { backend });
}

function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const info = providerErrorInfo(result);
  if (info) return [classifyProviderError(info, backend)];
  if (extractStatus(result) === 'empty') {
    return [
      diagnostic('zeroMatches', 'Query ran and matched nothing.', {
        backend,
        severity: 'info',
        blocksAnswer: false,
      }),
    ];
  }
  return [];
}

/**
 * Status + gated empty-provider diagnostics. A provider failure (rate limit,
 * auth, invalid query) already explains the empty result — emitting
 * providerUnindexed on top would misread a 403/429 as "repo not indexed".
 */
export function providerDiagnostics(
  result: CallToolResult,
  rowCount: number,
  backend: string
): OqlDiagnostic[] {
  const status = statusDiagnostics(result, backend);
  if (status.some(d => d.severity !== 'info')) return status;
  return [...status, ...emptyProviderDiag(rowCount, backend)];
}
