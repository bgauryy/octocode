import { generateCacheKey, withDataCache } from '../http/cache.js';
import { logPackageRegistryFailure } from '../../session.js';
import type {
  PackageSearchAPIResult,
  PackageSearchError,
  MinimalPackageResult,
  PythonPackageResult,
} from './types.js';

const MAX_DESCRIPTION_LENGTH = 200;
const MAX_KEYWORDS = 10;

function recordPyPiRegistryFailure(): void {
  try {
    logPackageRegistryFailure('pypi');
  } catch {
    // Local stats are best-effort and must not affect package search.
  }
}

async function searchPythonPackageInternal(
  packageName: string,
  fetchMetadata: boolean
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const normalizedName = packageName.toLowerCase().replace(/_/g, '-');

  const namesToTry = [
    normalizedName,
    packageName.toLowerCase(),
    packageName,
    packageName.replace(/-/g, '_').toLowerCase(),
  ];

  const uniqueNames = [...new Set(namesToTry)];

  for (const nameToTry of uniqueNames) {
    try {
      const encodedName = encodeURIComponent(nameToTry);
      const response = await fetch(
        `https://pypi.org/pypi/${encodedName}/json`,
        {
          headers: {
            Accept: 'application/json',
            'User-Agent': 'octocode-mcp/13.0.0',
          },
          signal: AbortSignal.timeout(15000),
        }
      );

      if (!response.ok) {
        if (response.status === 404) continue;
        recordPyPiRegistryFailure();
        throw new Error(`PyPI returned ${response.status}`);
      }

      const rawBody = await response.text();
      let packageInfo: any;
      try {
        packageInfo = JSON.parse(rawBody);
      } catch {
        continue;
      }
      const rawResponseChars = rawBody.length;
      if (!packageInfo.info) {
        continue;
      }

      const info = packageInfo.info;
      let repositoryUrl: string | null = null;
      if (info.project_urls) {
        const urlKeys = [
          'source',
          'repository',
          'homepage',
          'source code',
          'github',
          'gitlab',
        ];
        const projectUrlKeys = Object.keys(info.project_urls);
        for (const targetKey of urlKeys) {
          const matchedKey = projectUrlKeys.find(
            k => k.toLowerCase() === targetKey
          );
          if (matchedKey) {
            const url = info.project_urls[matchedKey];
            if (
              url &&
              (url.includes('github') ||
                url.includes('gitlab') ||
                url.includes('bitbucket'))
            ) {
              repositoryUrl = url;
              break;
            }
          }
        }
      }

      if (!repositoryUrl && info.home_page) {
        const homeUrl = info.home_page;
        if (
          homeUrl.includes('github') ||
          homeUrl.includes('gitlab') ||
          homeUrl.includes('bitbucket')
        ) {
          repositoryUrl = homeUrl;
        }
      }

      if (!fetchMetadata) {
        const minimalResult: MinimalPackageResult = {
          name: info.name || packageName,
          repository: repositoryUrl,
        };

        return {
          packages: [minimalResult],
          ecosystem: 'python',
          totalFound: 1,
          rawResponseChars,
        };
      }

      let keywords: string[] = [];
      if (info.keywords) {
        if (typeof info.keywords === 'string') {
          keywords = info.keywords
            .split(/[,\s]+/)
            .filter((k: string) => k.trim());
        } else if (Array.isArray(info.keywords)) {
          keywords = info.keywords;
        }
      }
      keywords = keywords.slice(0, MAX_KEYWORDS);

      let description = info.summary || info.description || null;
      if (description && description.length > MAX_DESCRIPTION_LENGTH) {
        description = description.substring(0, MAX_DESCRIPTION_LENGTH) + '...';
      }

      let lastPublished: string | undefined;
      const releases = packageInfo.releases;
      if (releases && info.version && releases[info.version]) {
        const versionFiles = releases[info.version];
        if (Array.isArray(versionFiles) && versionFiles.length > 0) {
          const uploadTime = versionFiles[0]?.upload_time;
          if (uploadTime) {
            lastPublished = uploadTime;
          }
        }
      }

      const result: PythonPackageResult = {
        name: info.name || packageName,
        version: info.version || 'latest',
        description,
        keywords,
        repository: repositoryUrl,
        homepage: info.home_page || undefined,
        author: info.author || undefined,
        license: info.license || undefined,
        lastPublished,
      };

      return {
        packages: [result],
        ecosystem: 'python',
        totalFound: 1,
        rawResponseChars,
      };
    } catch (error) {
      if (error instanceof Error && error.message.includes('404')) {
        continue;
      }
      throw error;
    }
  }

  return {
    packages: [],
    ecosystem: 'python',
    totalFound: 0,
  };
}

export async function searchPythonPackage(
  packageName: string,
  fetchMetadata: boolean
): Promise<PackageSearchAPIResult | PackageSearchError> {
  const cacheKey = generateCacheKey('pypi-search', {
    name: packageName,
    metadata: fetchMetadata,
  });

  return withDataCache(
    cacheKey,
    async () => {
      return searchPythonPackageInternal(packageName, fetchMetadata);
    },
    {
      shouldCache: result => !('error' in result),
    }
  );
}
