import { executeNpmCommand } from '../../exec/npm.js';
import { fetchWithRetries } from '../../http/fetch.js';
import type {
  NpmPackageResult,
  NpmSearchAPIResult,
  NpmSearchError,
} from '../types.js';
import { NpmViewResultSchema } from '../schemas.js';
import { getNpmRegistryUrl, type NpmViewResult } from './npmRegistry.js';
import {
  countRawPayloadChars,
  encodeRegistryPackageName,
  fetchWeeklyDownloads,
  mapToResult,
} from './npmMappers.js';

const NPM_VIEW_TIMEOUT_MS = 3000;

export function isNetworkFetchError(error: string | undefined): boolean {
  if (!error) return false;
  const lower = error.toLowerCase();
  return (
    lower.includes('fetch failed') ||
    lower.includes('failed to fetch') ||
    lower.includes('network') ||
    lower.includes('econnrefused') ||
    lower.includes('enotfound') ||
    lower.includes('etimedout') ||
    lower.includes('socket hang up') ||
    lower.includes('connect timeout') ||
    lower.includes('command timeout') ||
    lower.includes('circuit open') ||
    lower.includes('circuit breaker')
  );
}

export async function fetchLastPublished(
  packageName: string
): Promise<{ lastPublished?: string; rawResponseChars: number }> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const urlName = encodeRegistryPackageName(packageName);
    const url = `${registryUrl}/${urlName}`;
    const signal = AbortSignal.timeout(8000);

    try {
      const data = (await fetchWithRetries(url, {
        maxRetries: 0,
        initialDelayMs: 300,
        headers: { Accept: 'application/vnd.npm.install-v1+json' },
        signal,
        packageRegistry: 'npm',
      })) as { modified?: string } | null;
      const rawResponseChars = countRawPayloadChars(data);
      if (data?.modified)
        return { lastPublished: data.modified, rawResponseChars };
    } catch {
      void 0;
    }

    const data = (await fetchWithRetries(url, {
      maxRetries: 0,
      initialDelayMs: 300,
      headers: { Accept: 'application/json' },
      signal,
      packageRegistry: 'npm',
    })) as { time?: { modified?: string } } | null;
    return {
      lastPublished: data?.time?.modified || undefined,
      rawResponseChars: countRawPayloadChars(data),
    };
  } catch {
    return { rawResponseChars: 0 };
  }
}

export type PackageDetailsLookupResult = {
  pkg: NpmPackageResult | null;
  errorDetail?: string;
  rawResponseChars: number;
};

export async function enrichPackageDetails(
  packageName: string,
  pkg: NpmPackageResult,
  rawResponseChars: number
): Promise<PackageDetailsLookupResult> {
  const [downloadsResult, lastPublishedResult] = await Promise.all([
    fetchWeeklyDownloads(packageName),
    pkg.lastPublished
      ? Promise.resolve({ lastPublished: undefined, rawResponseChars: 0 })
      : fetchLastPublished(packageName),
  ]);
  if (downloadsResult.downloads !== undefined) {
    pkg.weeklyDownloads = downloadsResult.downloads;
  }
  if (lastPublishedResult.lastPublished && !pkg.lastPublished) {
    pkg.lastPublished = lastPublishedResult.lastPublished;
  }

  return {
    pkg,
    rawResponseChars:
      rawResponseChars +
      downloadsResult.rawResponseChars +
      lastPublishedResult.rawResponseChars,
  };
}

function isNotFoundMessage(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes('404') ||
    lower.includes('not found') ||
    lower.includes('e404')
  );
}

async function fetchPackageDetailsFromCli(
  packageName: string,
  includeExtendedMetadata: boolean
): Promise<PackageDetailsLookupResult> {
  try {
    const result = await executeNpmCommand('view', [packageName, '--json'], {
      timeout: NPM_VIEW_TIMEOUT_MS,
    });
    if (!result) return { pkg: null, rawResponseChars: 0 };
    if (result.error || result.exitCode !== 0) {
      const msg =
        result.error?.message ||
        result.stderr ||
        `npm exited ${result.exitCode}`;
      return {
        pkg: null,
        ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
        rawResponseChars: countRawPayloadChars(result.stdout),
      };
    }

    const output = result.stdout.trim();
    if (!output || output === 'undefined') {
      return { pkg: null, rawResponseChars: 0 };
    }

    let raw: unknown;
    try {
      raw = JSON.parse(output);
    } catch {
      return {
        pkg: null,
        errorDetail: 'Invalid npm view JSON output',
        rawResponseChars: output.length,
      };
    }

    const rawResponseChars = countRawPayloadChars(raw);
    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success) {
      return {
        pkg: null,
        errorDetail: 'Invalid npm view response format',
        rawResponseChars,
      };
    }

    const pkg = mapToResult(
      validation.data as NpmViewResult,
      includeExtendedMetadata,
      'cli'
    );
    return { pkg, rawResponseChars };
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return {
      pkg: null,
      ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
      rawResponseChars: 0,
    };
  }
}

async function fetchPackageDetailsFromRegistry(
  packageName: string,
  includeExtendedMetadata: boolean
): Promise<PackageDetailsLookupResult> {
  try {
    const registryUrl = await getNpmRegistryUrl();
    const urlName = encodeRegistryPackageName(packageName);
    const url = `${registryUrl}/${urlName}/latest`;

    let raw: unknown;
    try {
      raw = await fetchWithRetries(url, {
        maxRetries: 1,
        initialDelayMs: 500,
        headers: { Accept: 'application/json' },
        signal: AbortSignal.timeout(8000),
        packageRegistry: 'npm',
      });
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      if (msg.includes('404') || msg.toLowerCase().includes('not found')) {
        return { pkg: null, rawResponseChars: 0 };
      }
      return { pkg: null, errorDetail: msg, rawResponseChars: 0 };
    }

    const rawResponseChars = countRawPayloadChars(raw);

    if (!raw || typeof raw !== 'object') {
      return { pkg: null, rawResponseChars };
    }

    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success) {
      return {
        pkg: null,
        errorDetail: 'Invalid npm registry response format',
        rawResponseChars,
      };
    }

    const pkg = mapToResult(
      validation.data as NpmViewResult,
      includeExtendedMetadata,
      'registry'
    );

    return { pkg, rawResponseChars };
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    return {
      pkg: null,
      ...(isNotFoundMessage(msg) ? {} : { errorDetail: msg }),
      rawResponseChars: 0,
    };
  }
}

function cdnPackageJsonUrls(packageName: string): string[] {
  return [
    `https://cdn.jsdelivr.net/npm/${packageName}/package.json`,
    `https://unpkg.com/${packageName}/package.json`,
  ];
}

export async function fetchPackageDetailsFromCdn(
  packageName: string,
  includeExtendedMetadata: boolean
): Promise<PackageDetailsLookupResult> {
  let rawResponseChars = 0;
  let errorDetail: string | undefined;

  for (const url of cdnPackageJsonUrls(packageName)) {
    let raw: unknown;
    try {
      raw = await fetchWithRetries(url, {
        maxRetries: 0,
        initialDelayMs: 300,
        headers: { Accept: 'application/json' },
        signal: AbortSignal.timeout(8000),
      });
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      if (!isNotFoundMessage(msg)) {
        errorDetail = msg;
      }
      continue;
    }

    rawResponseChars += countRawPayloadChars(raw);
    if (!raw || typeof raw !== 'object') {
      continue;
    }

    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success) {
      errorDetail = 'Invalid npm CDN package.json response format';
      continue;
    }

    const pkg = mapToResult(
      validation.data as NpmViewResult,
      includeExtendedMetadata,
      'cdn'
    );

    return { pkg, rawResponseChars };
  }

  return {
    pkg: null,
    ...(errorDetail ? { errorDetail } : {}),
    rawResponseChars,
  };
}

export async function fetchPackageDetailsWithError(
  packageName: string,
  includeExtendedMetadata: boolean = false
): Promise<PackageDetailsLookupResult> {
  const [cliSettled, regSettled] = await Promise.allSettled([
    fetchPackageDetailsFromCli(packageName, includeExtendedMetadata),
    fetchPackageDetailsFromRegistry(packageName, includeExtendedMetadata),
  ]);

  /* c8 ignore next */
  const cliResult =
    cliSettled.status === 'fulfilled'
      ? cliSettled.value
      : { pkg: null, rawResponseChars: 0 };
  /* c8 ignore next */
  const regResult =
    regSettled.status === 'fulfilled'
      ? regSettled.value
      : { pkg: null, rawResponseChars: 0 };

  const winner = cliResult.pkg ? cliResult : regResult.pkg ? regResult : null;
  if (winner?.pkg) {
    return enrichPackageDetails(
      packageName,
      winner.pkg,
      winner.rawResponseChars
    );
  }

  const errorDetail =
    (cliResult as PackageDetailsLookupResult).errorDetail ||
    (regResult as PackageDetailsLookupResult).errorDetail;
  return {
    pkg: null,
    errorDetail,
    rawResponseChars: cliResult.rawResponseChars + regResult.rawResponseChars,
  };
}

export async function fetchNpmPackageByView(
  packageName: string,
  fetchMetadata: boolean,
  allowCdnFallback: boolean = false
): Promise<NpmSearchAPIResult | NpmSearchError> {
  const { pkg, errorDetail, rawResponseChars } =
    await fetchPackageDetailsWithError(packageName, fetchMetadata);

  if (!pkg) {
    if (errorDetail) {
      const isNetwork = isNetworkFetchError(errorDetail);
      if (isNetwork && allowCdnFallback) {
        const cdnResult = await fetchPackageDetailsFromCdn(
          packageName,
          fetchMetadata
        );
        if (cdnResult.pkg) {
          const enriched = await enrichPackageDetails(
            packageName,
            cdnResult.pkg,
            cdnResult.rawResponseChars
          );
          return {
            packages: enriched.pkg ? [enriched.pkg] : [],
            totalFound: enriched.pkg ? 1 : 0,
            rawResponseChars: rawResponseChars + enriched.rawResponseChars,
          };
        }
      }
      return {
        error: `NPM view failed for '${packageName}': ${errorDetail}`,
        rawResponseChars,
        hints: isNetwork
          ? [
              'npm registry is unreachable.',
              'Use `ghSearchRepos` to find the source repo directly by package name or domain terms.',
            ]
          : [
              'Ensure npm is installed and available in PATH',
              'Check package name for typos',
              `Try: npm view ${packageName} --json`,
            ],
      };
    }
    return {
      packages: [],
      totalFound: 0,
      rawResponseChars,
    };
  }

  return {
    packages: [pkg],
    totalFound: 1,
    rawResponseChars,
  };
}
