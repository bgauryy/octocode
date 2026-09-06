import type {
  NpmPackageResult,
  NpmSearchAPIResult,
  NpmSearchError,
} from '../types.js';
import { NpmRegistrySearchSchema } from '../schemas.js';
import {
  fetchNpmRegistryJson,
  type NpmRegistryContext,
  type NpmRegistrySearchItem,
} from './npmRegistry.js';
import {
  cleanRepoUrl,
  countRawPayloadChars,
  parseRegistrySearchTotal,
} from './npmMappers.js';
import { fetchNpmPackage } from './npmDetailsFetchers.js';

export async function searchNpmPackageViaRegistrySearch(
  keywords: string,
  limit: number,
  fetchMetadata: boolean,
  from: number,
  context: NpmRegistryContext
): Promise<NpmSearchAPIResult | NpmSearchError> {
  try {
    const fromParam = from > 0 ? `&from=${from}` : '';
    const url = `-/v1/search?text=${encodeURIComponent(keywords)}&size=${limit}${fromParam}`;

    let raw: unknown;
    try {
      raw = await fetchNpmRegistryJson(context, url);
    } catch (fetchErr) {
      const msg =
        fetchErr instanceof Error ? fetchErr.message : String(fetchErr);
      return {
        error: `Could not reach the npm search API (${msg}). The package may not exist, or the registry search endpoint is unavailable.`,
        hints: [
          'Verify the exact package name / scope (e.g. "@scope/name")',
          'For an exact known name, query it directly instead of a keyword search',
          'Ensure the npm registry is accessible from this environment',
        ],
      };
    }

    const searchRawResponseChars = countRawPayloadChars(raw);

    if (!raw || typeof raw !== 'object') {
      return {
        error:
          'Invalid npm registry search response; expected an object with results and a total.',
        rawResponseChars: searchRawResponseChars,
      };
    }

    const validation = NpmRegistrySearchSchema.safeParse(raw);
    if (!validation.success) {
      const issues = validation.error.issues.map(i => i.message).join('; ');
      return {
        error: `Invalid npm registry search response format: ${issues}`,
        rawResponseChars: searchRawResponseChars,
        hints: [
          'Try a different search term',
          'Use packageName for an exact package lookup',
        ],
      };
    }

    const totalFound = parseRegistrySearchTotal(validation.data.total, NaN);
    if (!Number.isSafeInteger(totalFound) || totalFound < 0) {
      return {
        error:
          'npm registry search omitted a valid total; pagination cannot be determined.',
        rawResponseChars: searchRawResponseChars,
      };
    }
    if (validation.data.objects.some(obj => !obj.package.name)) {
      return {
        error:
          'npm registry search returned an unnamed package; refusing to skip a result.',
        rawResponseChars: searchRawResponseChars,
      };
    }

    const searchResults = (
      validation.data.objects
        .map(obj => obj.package as NpmRegistrySearchItem)
        .filter(
          (pkg): pkg is NpmRegistrySearchItem & { name: string } =>
            typeof pkg.name === 'string' && pkg.name.length > 0
        ) as (NpmRegistrySearchItem & { name: string })[]
    ).slice(0, limit);

    const packageResults = await Promise.all(
      searchResults.map(async item => {
        if (fetchMetadata) {
          const detailsResult = await fetchNpmPackage(item.name, true, context);
          if (!('error' in detailsResult) && detailsResult.packages[0])
            return {
              pkg: detailsResult.packages[0] as NpmPackageResult,
              rawResponseChars: detailsResult.rawResponseChars ?? 0,
            };
        }

        return {
          pkg: {
            name: item.name,
            npmUrl:
              (item.links?.npm ?? '') ||
              `https://www.npmjs.com/package/${encodeURIComponent(item.name)}`,
            repoUrl:
              item.links?.repository &&
              typeof item.links.repository === 'string'
                ? cleanRepoUrl(item.links.repository)
                : null,
            version: item.version ?? 'unknown',
            source: 'registry' as const,
            ...(item.description
              ? { description: item.description as string }
              : {}),
            ...(typeof item.license === 'string'
              ? { license: item.license }
              : {}),
            ...(typeof item.date === 'string'
              ? { lastPublished: item.date }
              : {}),
            ...(item.links?.homepage
              ? { homepage: item.links.homepage as string }
              : {}),
          } as NpmPackageResult,
          rawResponseChars: 0,
        };
      })
    );
    const packages = packageResults
      .map(result => result.pkg)
      .filter((pkg): pkg is NpmPackageResult => Boolean(pkg));
    const detailRawResponseChars = packageResults.reduce(
      (sum, result) => sum + result.rawResponseChars,
      0
    );

    return {
      packages,
      totalFound,
      rawResponseChars: searchRawResponseChars + detailRawResponseChars,
    };
    /* c8 ignore next */
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    return {
      error: `NPM registry search failed: ${errorMsg}`,
      hints: [
        'Check package name for typos',
        'Try searching with a simpler term',
        'Ensure npm registry is accessible',
      ],
    };
  }
}
