import type { GitHubApiFileItem } from '../tools/github_view_repo_structure/types.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';
import { OctokitWithThrottling } from './client.js';

export async function fetchDirectoryContentsRecursivelyAPI(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  branch: string,
  path: string,
  currentDepth: number,
  maxDepth: number,
  visitedPaths: Set<string> = new Set()
): Promise<GitHubApiFileItem[]> {
  if (currentDepth > maxDepth || visitedPaths.has(path)) {
    return attachRawResponseChars([], 0);
  }

  visitedPaths.add(path);

  try {
    const result = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: path || '',
      ref: branch,
    });

    let rawResponseChars = countSerializedChars(result.data);
    const items = Array.isArray(result.data) ? result.data : [result.data];
    const apiItems = items.map((item: GitHubApiFileItem) => ({
      name: item.name,
      path: item.path,
      type: item.type as 'file' | 'dir',
      size: 'size' in item ? item.size : undefined,
      download_url: 'download_url' in item ? item.download_url : undefined,
      url: item.url,
      html_url: item.html_url,
      git_url: item.git_url,
      sha: item.sha,
    }));

    const allItems: GitHubApiFileItem[] = [
      ...apiItems,
    ] as unknown as GitHubApiFileItem[];

    if (currentDepth < maxDepth) {
      const directories = apiItems.filter(item => item.type === 'dir');

      const concurrencyLimit = 3;
      for (let i = 0; i < directories.length; i += concurrencyLimit) {
        const batch = directories.slice(i, i + concurrencyLimit);

        const promises = batch.map(async dir => {
          try {
            const subItems = await fetchDirectoryContentsRecursivelyAPI(
              octokit,
              owner,
              repo,
              branch,
              dir.path,
              currentDepth + 1,
              maxDepth,
              visitedPaths
            );
            rawResponseChars += getRawResponseChars(subItems) ?? 0;
            return subItems;
          } catch {
            return [];
          }
        });

        const results = await Promise.all(promises);
        results.forEach(subItems => {
          allItems.push(...subItems);
        });
      }
    }

    return attachRawResponseChars(allItems, rawResponseChars);
  } catch {
    return attachRawResponseChars([], 0);
  }
}
