import type { Octokit } from 'octokit';
import type { GitHubApiFileItem } from '../tools/github_view_repo_structure/types.js';
import { countSerializedChars } from '../utils/response/charSavings.js';

export type GitTreeEntry = {
  path?: string;
  mode?: string;
  type?: string;
  sha?: string;
  size?: number;
  url?: string;
};

export type FilteredGitTree = {
  items: GitHubApiFileItem[];
  truncated: boolean;
  rawResponseChars: number;
};

/**
 * Map a recursive Git Trees payload into structure listing items, scoped to
 * `pathPrefix` and capped at `maxDepth` (1 = immediate children only).
 */
export function filterGitTreeEntries(
  tree: GitTreeEntry[],
  options: { pathPrefix?: string; maxDepth: number }
): GitHubApiFileItem[] {
  const prefix = (options.pathPrefix ?? '').replace(/^\/+|\/+$/g, '');
  const prefixWithSlash = prefix ? `${prefix}/` : '';
  const maxDepth = Math.max(1, options.maxDepth);
  const items: GitHubApiFileItem[] = [];

  for (const entry of tree) {
    if (!entry.path || (entry.type !== 'blob' && entry.type !== 'tree')) {
      continue;
    }

    let relative = entry.path;
    if (prefix) {
      if (entry.path === prefix) continue;
      if (!entry.path.startsWith(prefixWithSlash)) continue;
      relative = entry.path.slice(prefixWithSlash.length);
    }

    const depth = relative.split('/').filter(Boolean).length;
    if (depth === 0 || depth > maxDepth) continue;

    const slash = relative.lastIndexOf('/');
    const name = slash === -1 ? relative : relative.slice(slash + 1);

    items.push({
      name,
      path: entry.path,
      type: entry.type === 'tree' ? 'dir' : 'file',
      size: entry.type === 'blob' ? entry.size : undefined,
      sha: entry.sha,
      url: entry.url,
      html_url: undefined,
      git_url: undefined,
      download_url: undefined,
    } as GitHubApiFileItem);
  }

  return items;
}

export function isGitStructureTreesEnabled(
  env: NodeJS.ProcessEnv = process.env
): boolean {
  return env.OCTOCODE_GH_STRUCTURE_TREES !== '0';
}

/**
 * Fetch a repo tree via `git.getTree({ recursive: 'true' })` and filter to the
 * requested path/depth. Resolves branch → tree SHA in two REST calls total.
 */
export async function fetchStructureViaGitTree(
  octokit: Octokit,
  params: {
    owner: string;
    repo: string;
    workingBranch: string;
    pathPrefix?: string;
    maxDepth: number;
  }
): Promise<FilteredGitTree> {
  const { data: branchData } = await octokit.rest.repos.getBranch({
    owner: params.owner,
    repo: params.repo,
    branch: params.workingBranch,
  });

  const treeSha = branchData.commit.commit.tree.sha;
  if (!treeSha) {
    throw new Error(
      `Could not resolve tree SHA for ${params.owner}/${params.repo}@${params.workingBranch}`
    );
  }

  const { data: treeData } = await octokit.rest.git.getTree({
    owner: params.owner,
    repo: params.repo,
    tree_sha: treeSha,
    recursive: 'true',
  });

  const items = filterGitTreeEntries(treeData.tree as GitTreeEntry[], {
    pathPrefix: params.pathPrefix,
    maxDepth: params.maxDepth,
  });

  return {
    items,
    truncated: Boolean(treeData.truncated),
    rawResponseChars:
      countSerializedChars(branchData) + countSerializedChars(treeData),
  };
}
