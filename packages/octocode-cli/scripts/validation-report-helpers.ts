import { resolveTokenFull } from 'octocode-shared';

export type ValidationStatus = 'valid' | 'invalid' | 'error' | 'warning';

export interface BaseValidationResult {
  id: string;
  name: string;
  status: ValidationStatus;
  error?: string;
  statusCode?: number;
  stars?: number;
  lastPushed?: string;
}

export interface ValidationBuckets<T extends BaseValidationResult> {
  valid: T[];
  warnings: T[];
  invalid: T[];
  errors: T[];
  blocking: T[];
  staleWarnings: T[];
  otherWarnings: T[];
}

export interface GitHubRepoInfo {
  id: number;
  name: string;
  full_name: string;
  private: boolean;
  html_url: string;
  description: string | null;
  archived: boolean;
  disabled: boolean;
  stargazers_count: number;
  pushed_at: string;
}

export interface GitHubRepositoryCheckResult {
  exists: boolean;
  error?: string;
  statusCode?: number;
  data?: GitHubRepoInfo;
}

const REPORT_WIDTH = 80;
const BANNER_INNER_WIDTH = 79;
const STALE_REPOSITORY_WARNING = 'not been updated in over 1 year';

export async function resolveValidatorToken(): Promise<string | null> {
  try {
    const result = await resolveTokenFull();
    return result?.token || null;
  } catch {
    return null;
  }
}

export function buildGitHubApiHeaders(
  userAgent: string,
  token?: string | null
): Record<string, string> {
  const headers: Record<string, string> = {
    Accept: 'application/vnd.github.v3+json',
    'User-Agent': userAgent,
  };

  if (token) {
    headers.Authorization = `token ${token}`;
  } else if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `token ${process.env.GITHUB_TOKEN}`;
  } else if (process.env.GITHUB_PERSONAL_ACCESS_TOKEN) {
    headers.Authorization = `token ${process.env.GITHUB_PERSONAL_ACCESS_TOKEN}`;
  }

  return headers;
}

export async function checkGitHubRepository(
  owner: string,
  repo: string,
  userAgent: string,
  token?: string | null
): Promise<GitHubRepositoryCheckResult> {
  const url = `https://api.github.com/repos/${owner}/${repo}`;

  try {
    const response = await fetch(url, {
      headers: buildGitHubApiHeaders(userAgent, token),
    });

    if (response.ok) {
      const data = (await response.json()) as GitHubRepoInfo;
      return { exists: true, statusCode: response.status, data };
    }

    if (response.status === 404) {
      return {
        exists: false,
        error: 'Repository not found',
        statusCode: response.status,
      };
    }

    if (response.status === 403) {
      const remaining = response.headers.get('x-ratelimit-remaining');
      if (remaining === '0') {
        return {
          exists: false,
          error:
            'Rate limit exceeded. Set GITHUB_TOKEN env var for higher limits.',
          statusCode: response.status,
        };
      }
      return {
        exists: false,
        error: 'Access forbidden',
        statusCode: response.status,
      };
    }

    return {
      exists: false,
      error: `HTTP ${response.status}: ${response.statusText}`,
      statusCode: response.status,
    };
  } catch (err) {
    return {
      exists: false,
      error: err instanceof Error ? err.message : 'Unknown error',
    };
  }
}

export function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays < 30) return `${diffDays} days ago`;
  if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
  return `${(diffDays / 365).toFixed(1)} years ago`;
}

export function isStaleWarning(result: BaseValidationResult): boolean {
  return result.error?.includes(STALE_REPOSITORY_WARNING) === true;
}

export function splitValidationResults<T extends BaseValidationResult>(
  results: T[]
): ValidationBuckets<T> {
  const valid: T[] = [];
  const warnings: T[] = [];
  const invalid: T[] = [];
  const errors: T[] = [];

  for (const result of results) {
    switch (result.status) {
      case 'valid':
        valid.push(result);
        break;
      case 'warning':
        warnings.push(result);
        break;
      case 'invalid':
        invalid.push(result);
        break;
      case 'error':
        errors.push(result);
        break;
    }
  }

  const staleWarnings = warnings.filter(isStaleWarning);
  const otherWarnings = warnings.filter(result => !isStaleWarning(result));

  return {
    valid,
    warnings,
    invalid,
    errors,
    blocking: [...invalid, ...errors],
    staleWarnings,
    otherWarnings,
  };
}

export function writeValidationProgress<T extends BaseValidationResult>(
  results: T[],
  progress: number,
  total: number
): void {
  const { valid, warnings, invalid, errors } = splitValidationResults(results);

  process.stdout.write(
    `\r  Progress: ${progress}/${total} | ✅ ${valid.length} | ⚠️  ${warnings.length} | ❌ ${invalid.length} | 🔴 ${errors.length}`
  );
}

export function printValidatorBanner(title: string): void {
  console.log(`╔${'═'.repeat(BANNER_INNER_WIDTH)}╗`);
  console.log(`║${center(title, BANNER_INNER_WIDTH)}║`);
  console.log(`╚${'═'.repeat(BANNER_INNER_WIDTH)}╝`);
}

export function printRateLimitTip(): void {
  console.log(
    '\n⚠️  TIP: Set GITHUB_TOKEN or GITHUB_PERSONAL_ACCESS_TOKEN for higher rate limits\n'
  );
}

export function printReportHeader(title: string): void {
  console.log('═'.repeat(REPORT_WIDTH));
  console.log(center(title, REPORT_WIDTH));
  console.log('═'.repeat(REPORT_WIDTH));
  console.log();
}

export function printSectionHeader(title: string, width = REPORT_WIDTH): void {
  console.log(title);
  console.log('─'.repeat(width));
}

export function printSummary(
  rows: Array<[label: string, value: string | number]>
): void {
  console.log('📊 SUMMARY');
  console.log('─'.repeat(40));

  const labelWidth = Math.max(...rows.map(([label]) => label.length));
  for (const [label, value] of rows) {
    console.log(`  ${label.padEnd(labelWidth)} ${value}`);
  }

  console.log();
}

export function topByStars<T extends BaseValidationResult>(
  results: T[],
  limit?: number
): T[] {
  const sorted = [...results]
    .filter(result => result.stars !== undefined)
    .sort((a, b) => (b.stars ?? 0) - (a.stars ?? 0));

  return limit === undefined ? sorted : sorted.slice(0, limit);
}

export function buildValidationJsonSummary<
  T extends BaseValidationResult,
  TExtra extends Record<string, unknown> = Record<string, never>,
>(
  results: T[],
  extra?: TExtra
): {
  invalid: T[];
  warnings: T[];
  total: number;
  validCount: number;
} & TExtra {
  const { valid, warnings, blocking } = splitValidationResults(results);

  return {
    invalid: blocking,
    warnings,
    total: results.length,
    validCount: valid.length,
    ...(extra ?? ({} as TExtra)),
  };
}

export function hasBlockingValidationFailures<T extends BaseValidationResult>(
  results: T[]
): boolean {
  const { blocking } = splitValidationResults(results);
  return blocking.length > 0;
}

function center(text: string, width: number): string {
  if (text.length >= width) {
    return text.slice(0, width);
  }

  const leftPadding = Math.floor((width - text.length) / 2);
  const rightPadding = width - text.length - leftPadding;
  return `${' '.repeat(leftPadding)}${text}${' '.repeat(rightPadding)}`;
}
