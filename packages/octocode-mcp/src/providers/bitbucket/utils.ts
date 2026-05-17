import type { ProviderResponse } from '../types.js';
import type {
  BitbucketAPIError,
  BitbucketAPIResponse,
} from '../../bitbucket/types.js';
import { countSerializedChars } from '../../utils/response/charSavings.js';
import { logRateLimit } from '../../session.js';

interface BitbucketProjectId {
  workspace: string;
  repoSlug: string;
}

export function parseBitbucketProjectId(
  projectId?: string
): BitbucketProjectId {
  if (!projectId) {
    throw new Error('Project ID is required');
  }

  const parts = projectId.split('/');
  if (parts.length !== 2 || !parts[0] || !parts[1]) {
    throw new Error(
      `Invalid Bitbucket projectId: '${projectId}'. Expected 'workspace/repo_slug'.`
    );
  }

  return { workspace: parts[0], repoSlug: parts[1] };
}

export function extractBitbucketRateLimit(
  apiError: BitbucketAPIError
): ProviderResponse<never>['rateLimit'] {
  if (
    apiError.rateLimitRemaining === undefined &&
    apiError.retryAfter === undefined &&
    apiError.rateLimitReset === undefined
  ) {
    return undefined;
  }

  const reset =
    apiError.rateLimitReset ??
    (apiError.retryAfter !== undefined
      ? Math.floor(Date.now() / 1000) + apiError.retryAfter
      : undefined);

  if (reset === undefined) {
    return undefined;
  }

  return {
    remaining: apiError.rateLimitRemaining ?? 0,
    reset,
    retryAfter: apiError.retryAfter,
  };
}

function recordBitbucketRateLimit(
  rateLimit: NonNullable<ProviderResponse<never>['rateLimit']>
): void {
  void logRateLimit({
    limit_type: 'primary',
    retry_after_seconds: rateLimit.retryAfter,
    rate_limit_remaining: rateLimit.remaining,
    rate_limit_reset_ms: rateLimit.reset * 1000,
    provider: 'bitbucket',
  });
}

export function handleBitbucketAPIResponse<TData, TRaw>(
  result: BitbucketAPIResponse<TRaw>,
  transform: (data: TRaw) => TData
): ProviderResponse<TData> {
  if ('error' in result && result.error) {
    const rateLimit = extractBitbucketRateLimit(result as BitbucketAPIError);
    if (rateLimit) {
      recordBitbucketRateLimit(rateLimit);
    }

    return {
      error:
        typeof result.error === 'string' ? result.error : String(result.error),
      status: result.status || 500,
      provider: 'bitbucket',
      hints:
        'hints' in result ? (result as BitbucketAPIError).hints : undefined,
      rateLimit,
    };
  }

  if (!('data' in result) || !result.data) {
    return {
      error: 'No data returned from Bitbucket API',
      status: 500,
      provider: 'bitbucket',
    };
  }

  return {
    data: transform(result.data),
    status: 200,
    provider: 'bitbucket',
    rawResponseChars: countSerializedChars(result.data),
  };
}
