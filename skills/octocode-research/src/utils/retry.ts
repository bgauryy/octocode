import {
  getErrorStatus,
  hasStatusIn,
  hasCodeIn,
  messageMatches,
} from '../types/errorGuards.js';

export interface RetryConfig {
  maxAttempts: number;
  initialDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
  retryOn: (error: unknown) => boolean;
}


export const RETRY_CONFIGS = {
  
  lsp: {
    maxAttempts: 3,
    initialDelayMs: 500,
    maxDelayMs: 5000,
    backoffMultiplier: 2,
    retryOn: (err: unknown) =>
      isLspNotReady(err) || isTimeout(err) || isConnectionRefused(err),
  },

  
  github: {
    maxAttempts: 3,
    initialDelayMs: 1000,
    maxDelayMs: 30000,
    backoffMultiplier: 3,
    retryOn: (err: unknown) =>
      isRateLimited(err) || isServerError(err) || isTimeout(err),
  },

  
  package: {
    maxAttempts: 3,
    initialDelayMs: 1000,
    maxDelayMs: 15000,
    backoffMultiplier: 2,
    retryOn: (err: unknown) =>
      isRateLimited(err) || isServerError(err) || isTimeout(err),
  },

  
  local: {
    maxAttempts: 2,
    initialDelayMs: 100,
    maxDelayMs: 1000,
    backoffMultiplier: 2,
    retryOn: (err: unknown) => isFileBusy(err) || isTimeout(err),
  },
} as const satisfies Record<string, RetryConfig>;


interface RetryContext {
  tool: string;
  params?: unknown;
}

export async function withRetry<T>(
  operation: () => Promise<T>,
  config: RetryConfig,
  context?: RetryContext
): Promise<T> {
  let lastError: unknown;
  let delay = config.initialDelayMs;

  for (let attempt = 1; attempt <= config.maxAttempts; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;

      if (!config.retryOn(error) || attempt === config.maxAttempts) {
        throw error;
      }

      const toolName = context?.tool || 'operation';
      console.log(
        `⟳ Retry ${attempt}/${config.maxAttempts} for ${toolName} in ${delay}ms`
      );

      await sleep(delay);
      delay = Math.min(delay * config.backoffMultiplier, config.maxDelayMs);
    }
  }

  throw lastError;
}


const RATE_LIMIT_CODES = [403, 429] as const;
const RATE_LIMIT_PATTERNS = [/rate\s*limit/i, /too\s*many\s*requests/i] as const;

const LSP_ERROR_CODES = ['LSP_NOT_READY', 'LSP_NOT_INITIALIZED', 'ECONNREFUSED'] as const;
const LSP_ERROR_PATTERNS = [/not initialized/i, /server not started/i, /lsp.*not.*ready/i] as const;

const TIMEOUT_CODES = ['ETIMEDOUT', 'ESOCKETTIMEDOUT', 'ECONNRESET'] as const;
const TIMEOUT_PATTERNS = [/timeout/i, /timed?\s*out/i] as const;

const FILE_BUSY_CODES = ['EBUSY', 'EAGAIN', 'ENOTEMPTY'] as const;

const CONNECTION_REFUSED_CODES = ['ECONNREFUSED', 'ENOTFOUND', 'EHOSTUNREACH'] as const;


function isRateLimited(err: unknown): boolean {
  if (hasStatusIn(err, RATE_LIMIT_CODES)) {
    return true;
  }

  return messageMatches(err, RATE_LIMIT_PATTERNS);
}


function isLspNotReady(err: unknown): boolean {
  if (hasCodeIn(err, LSP_ERROR_CODES)) {
    return true;
  }

  return messageMatches(err, LSP_ERROR_PATTERNS);
}


function isTimeout(err: unknown): boolean {
  if (hasCodeIn(err, TIMEOUT_CODES)) {
    return true;
  }

  return messageMatches(err, TIMEOUT_PATTERNS);
}


function isServerError(err: unknown): boolean {
  const status = getErrorStatus(err);
  return status !== undefined && status >= 500 && status < 600;
}


function isFileBusy(err: unknown): boolean {
  return hasCodeIn(err, FILE_BUSY_CODES);
}


function isConnectionRefused(err: unknown): boolean {
  return hasCodeIn(err, CONNECTION_REFUSED_CODES);
}


const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));
