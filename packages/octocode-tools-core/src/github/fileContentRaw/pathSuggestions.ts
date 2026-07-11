import type { OctokitWithThrottling } from '../client.js';
import type { GitHubApiFileItem } from '../../tools/github_view_repo_structure/types.js';

export function buildPathSuggestionHints(
  requestedPath: string,
  suggestions: string[]
): string[] {
  const targetName = requestedPath.split('/').pop() || '';
  const isCaseMismatch = suggestions.some(s => {
    const suggestedName = s.split('/').pop() || '';
    return (
      suggestedName.toLowerCase() === targetName.toLowerCase() &&
      suggestedName !== targetName
    );
  });

  const hints: string[] = [];
  if (isCaseMismatch) {
    hints.push(
      'GitHub Contents API paths are case-sensitive. Verify exact file casing with ghViewRepoStructure.'
    );
  }
  hints.push(`Did you mean: ${suggestions.join(', ')}?`);
  return hints;
}

export async function findPathSuggestions(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  filePath: string,
  branch: string
): Promise<string[]> {
  try {
    const parentPath = filePath.split('/').slice(0, -1).join('/');
    const targetName = filePath.split('/').pop();

    if (!targetName) return [];

    const parentContent = await octokit.rest.repos.getContent({
      owner,
      repo,
      path: parentPath,
      ref: branch,
    });

    if (!Array.isArray(parentContent.data)) return [];

    const files = parentContent.data as GitHubApiFileItem[];
    const suggestions: string[] = [];

    const caseMatch = files.find(
      f => f.name.toLowerCase() === targetName.toLowerCase()
    );
    if (caseMatch) suggestions.push(caseMatch.path);

    const nameNoExt = targetName.replace(/\.[^/.]+$/, '');
    const extMatches = files.filter(f => {
      if (f.name === targetName) return false;
      if (f.name.startsWith(nameNoExt + '.')) return true;
      return false;
    });
    extMatches.forEach(f => suggestions.push(f.path));

    if (suggestions.length === 0 && nameNoExt.length >= 3) {
      const prefixMatches = files.filter(f => {
        const fBase = f.name.replace(/\.[^/.]+$/, '');
        return (
          fBase !== nameNoExt &&
          fBase.length >= 3 &&
          (nameNoExt.startsWith(fBase) || fBase.startsWith(nameNoExt))
        );
      });
      prefixMatches.forEach(f => suggestions.push(f.path));
    }

    return Array.from(new Set(suggestions)).slice(0, 3);
  } catch {
    return [];
  }
}
