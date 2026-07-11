import { MAX_CONTENT_SIZE_BYTES, SKILLS_SH_API } from './constants.js';
import type { SkillsShResult, SkillsShSearchResponse } from './types.js';

export async function readSkillFromGitHub(
  owner: string,
  repo: string,
  skillPath: string,
  branch = 'main'
): Promise<string> {
  const normalized =
    skillPath.length === 0
      ? 'SKILL.md'
      : skillPath.endsWith('/SKILL.md')
        ? skillPath
        : skillPath.endsWith('SKILL.md')
          ? skillPath
          : `${skillPath}/SKILL.md`;

  const url = `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${normalized}`;
  const response = await fetch(url, {
    headers: { 'User-Agent': 'octocode' },
    signal: AbortSignal.timeout(8000),
  });

  if (response.status === 404) {
    if (branch === 'main') {
      return readSkillFromGitHub(owner, repo, skillPath, 'master');
    }
    throw new Error(
      `SKILL.md not found at ${owner}/${repo}/${normalized} (tried main and master)`
    );
  }

  if (!response.ok) {
    throw new Error(
      `Failed to fetch SKILL.md: ${response.status} ${response.statusText}`
    );
  }

  const content = await response.text();
  if (content.length > MAX_CONTENT_SIZE_BYTES) {
    throw new Error(`SKILL.md too large (${content.length} bytes)`);
  }
  return content;
}

export async function fetchSkillsShSearch(
  query: string,
  limit = 20
): Promise<SkillsShSearchResponse> {
  const url = `${SKILLS_SH_API}?q=${encodeURIComponent(query)}&limit=${Math.min(limit, 100)}`;

  const response = await fetch(url, {
    headers: {
      'User-Agent':
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:150.0) Gecko/20100101 Firefox/150.0',
      Accept: 'application/json',
    },
    signal: AbortSignal.timeout(8000),
  });

  if (!response.ok) {
    throw new Error(
      `skills.sh search failed: ${response.status} ${response.statusText}`
    );
  }

  const data = (await response.json()) as {
    skills: SkillsShResult[];
    count: number;
  };

  const sorted = [...(data.skills ?? [])].sort(
    (a, b) => b.installs - a.installs
  );

  return {
    results: sorted.slice(0, limit),
    count: data.count ?? sorted.length,
  };
}
