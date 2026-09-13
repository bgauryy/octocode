import { RequestError } from 'octokit';
import type { GitHubApiFileItem } from '../tools/github_view_repo_structure/types.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';
import { OctokitWithThrottling } from './client.js';
import { fetchStructureViaGitTree } from './repoStructureTree.js';
import { CONTENTS_DIRECTORY_LIMIT } from '../tools/github_view_repo_structure/constants.js';

const RECURSIVE_FETCH_FAILURES = Symbol.for('octocode.recursiveFetchFailures');
const CONTENTS_LIMIT_REACHED = Symbol.for('octocode.contentsLimitReached');

/**
 * HTTP statuses that must propagate rather than be swallowed into a
 * partial/empty tree: a rate-limited or auth-denied subtree is NOT the same
 * as an empty directory, and presenting it as such hides the real failure.
 */
const PROPAGATE_STATUSES = new Set([401, 403, 429]);

export function shouldPropagateStructureError(error: unknown): boolean {
  return error instanceof RequestError && PROPAGATE_STATUSES.has(error.status);
}

function attachFailureCount<T extends object>(
  result: T,
  failures: number,
  contentsLimitReached = false
): T {
  try {
    Object.defineProperty(result, RECURSIVE_FETCH_FAILURES, {
      value: failures,
      enumerable: false,
      configurable: true,
    });
    Object.defineProperty(result, CONTENTS_LIMIT_REACHED, {
      value: contentsLimitReached,
      enumerable: false,
      configurable: true,
    });
  } catch {
    void 0;
  }
  return result;
}

export function hasRecursiveContentsLimit(value: unknown): boolean {
  return (
    typeof value === 'object' &&
    value !== null &&
    (value as Record<typeof CONTENTS_LIMIT_REACHED, unknown>)[
      CONTENTS_LIMIT_REACHED
    ] === true
  );
}

/** A saturated Contents response cannot prove completeness; try the existing tree reader. */
export async function recoverContentsDirectory(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  params: { owner: string; repo: string; branch: string; path: string },
  rawEntryCount: number,
  contentsItems: GitHubApiFileItem[]
): Promise<{
  items: GitHubApiFileItem[];
  contentsLimitReached: boolean;
  rawResponseChars: number;
}> {
  if (rawEntryCount < CONTENTS_DIRECTORY_LIMIT) {
    return {
      items: contentsItems,
      contentsLimitReached: false,
      rawResponseChars: 0,
    };
  }
  let rawResponseChars = 0;
  try {
    const tree = await fetchStructureViaGitTree(octokit, {
      owner: params.owner,
      repo: params.repo,
      workingBranch: params.branch,
      pathPrefix: params.path,
      maxDepth: 1,
    });
    rawResponseChars = tree.rawResponseChars;
    if (!tree.truncated && !tree.notModified) {
      return {
        items: tree.items,
        contentsLimitReached: false,
        rawResponseChars,
      };
    }
  } catch (error) {
    if (shouldPropagateStructureError(error)) throw error;
  }
  // Retain every available Contents entry when recovery cannot prove completeness.
  return { items: contentsItems, contentsLimitReached: true, rawResponseChars };
}

/**
 * Number of subtrees that failed to fetch (non-propagating errors) while
 * building this recursive listing. Lets callers surface a "partial tree"
 * warning instead of presenting a partial tree as complete.
 */
export function getRecursiveFetchFailureCount(value: unknown): number {
  if (typeof value !== 'object' || value === null) return 0;
  const raw = (value as Record<typeof RECURSIVE_FETCH_FAILURES, unknown>)[
    RECURSIVE_FETCH_FAILURES
  ];
  return typeof raw === 'number' && Number.isFinite(raw) && raw > 0 ? raw : 0;
}

export async function fetchDirectoryContentsRecursivelyAPI(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  branch: string,
  path: string,
  currentDepth: number,
  maxDepth: number,
  visitedPaths: Set<string> = new Set(),
  preloaded?: { data: unknown }
): Promise<GitHubApiFileItem[]> {
  if (currentDepth > maxDepth || visitedPaths.has(path)) {
    return attachFailureCount(attachRawResponseChars([], 0), 0);
  }

  visitedPaths.add(path);

  let result = preloaded;
  try {
    result ??= await octokit.rest.repos.getContent({
      owner,
      repo,
      path: path || '',
      ref: branch,
    });
  } catch (error) {
    // Rate-limit / auth failures must propagate; other failures degrade to an
    // empty listing but are counted so callers can warn about a partial tree.
    if (shouldPropagateStructureError(error)) {
      throw error;
    }
    return attachFailureCount(attachRawResponseChars([], 0), 1);
  }

  let rawResponseChars = countSerializedChars(result.data);
  const items = Array.isArray(result.data) ? result.data : [result.data];

  // Narrow on the discriminant: only 'file' and 'dir' entries are real tree
  // nodes. Submodule/symlink entries must be dropped, not mislabeled as files.
  const mappedItems: GitHubApiFileItem[] = items
    .filter(item => item.type === 'file' || item.type === 'dir')
    .map(
      item =>
        ({
          name: item.name,
          path: item.path,
          type: item.type,
          size: 'size' in item ? item.size : undefined,
          download_url: 'download_url' in item ? item.download_url : undefined,
          url: item.url,
          html_url: item.html_url,
          git_url: item.git_url,
          sha: item.sha,
          // Projected view of the API entry; the omitted fields (_links, etc.)
          // are not part of the structure listing.
        }) as GitHubApiFileItem
    );

  const recovered = await recoverContentsDirectory(
    octokit,
    { owner, repo, branch, path },
    items.length,
    mappedItems
  );
  rawResponseChars += recovered.rawResponseChars;
  const apiItems = recovered.items;
  const allItems: GitHubApiFileItem[] = [...apiItems];
  let failures = 0;
  let contentsLimitReached = recovered.contentsLimitReached;

  if (currentDepth < maxDepth) {
    const directories = apiItems.filter(item => item.type === 'dir');

    const concurrencyLimit = 3;
    for (let i = 0; i < directories.length; i += concurrencyLimit) {
      const batch = directories.slice(i, i + concurrencyLimit);

      const settled = await Promise.allSettled(
        batch.map(dir =>
          fetchDirectoryContentsRecursivelyAPI(
            octokit,
            owner,
            repo,
            branch,
            dir.path,
            currentDepth + 1,
            maxDepth,
            visitedPaths
          )
        )
      );

      for (const outcome of settled) {
        if (outcome.status === 'fulfilled') {
          const subItems = outcome.value;
          rawResponseChars += getRawResponseChars(subItems) ?? 0;
          failures += getRecursiveFetchFailureCount(subItems);
          contentsLimitReached ||= hasRecursiveContentsLimit(subItems);
          allItems.push(...subItems);
        } else {
          // Rate-limit / auth failures are real and must propagate so the
          // caller does not present a truncated tree as complete.
          if (shouldPropagateStructureError(outcome.reason)) {
            throw outcome.reason;
          }
          failures += 1;
        }
      }
    }
  }

  return attachFailureCount(
    attachRawResponseChars(allItems, rawResponseChars),
    failures,
    contentsLimitReached
  );
}
