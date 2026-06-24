import type {
  MarketplaceSource,
  MarketplaceSkill,
} from '../configs/skills-marketplace.js';
import { isLocalSource } from '../configs/skills-marketplace.js';
import {
  dirExists,
  writeFileContent,
  readFileContent,
  fileExists,
} from './fs.js';
import { join, isAbsolute } from 'node:path';
import { mkdirSync, statSync, readdirSync, unlinkSync, rmSync } from 'node:fs';
import os from 'node:os';
import { trySafe } from './try-safe.js';
import {
  getSkillsSourcePath,
  getAvailableSkills,
  installSkillToDestination,
  isPathInside,
  resolveSkillDestination,
} from './skills.js';
import { parseSkillFrontmatter } from './parsers/frontmatter.js';
import { z } from '@octocodeai/octocode-tools-core/zod';

const CACHE_TTL_MS = 24 * 60 * 60 * 1000;

const CachedSkillsDataSchema = z.object({
  timestamp: z.number(),
  skills: z.array(z.object({}).passthrough()),
});

type CachedSkillsData = {
  timestamp: number;
  skills: MarketplaceSkill[];
};

function getCacheDir(): string {
  const home = os.homedir();

  const npmCacheDir = process.env.npm_config_cache || join(home, '.npm');
  const npmOctocodeCache = join(npmCacheDir, '_cacache', 'octocode-skills');

  const isWindows = os.platform() === 'win32';
  const fallbackCache = isWindows
    ? join(
        process.env.LOCALAPPDATA || join(home, 'AppData', 'Local'),
        'octocode',
        'cache',
        'skills'
      )
    : join(home, '.cache', 'octocode', 'skills');

  if (dirExists(npmCacheDir)) {
    return npmOctocodeCache;
  }

  return fallbackCache;
}

function ensureCacheDir(): string {
  const cacheDir = getCacheDir();
  if (!dirExists(cacheDir)) {
    mkdirSync(cacheDir, { recursive: true, mode: 0o700 });
  }
  return cacheDir;
}

function getCacheFilePath(source: MarketplaceSource): string {
  const cacheDir = ensureCacheDir();
  return join(cacheDir, `${source.id}.json`);
}

function isCacheValid(cacheFile: string): boolean {
  return trySafe(() => {
    if (!fileExists(cacheFile)) {
      return false;
    }
    const stats = statSync(cacheFile);
    const age = Date.now() - stats.mtimeMs;
    return age < CACHE_TTL_MS;
  }, false);
}

function readCachedSkills(
  source: MarketplaceSource
): MarketplaceSkill[] | null {
  return trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    if (!isCacheValid(cacheFile)) {
      return null;
    }
    const content = readFileContent(cacheFile);
    if (!content) {
      return null;
    }
    const raw = JSON.parse(content);
    const validated = CachedSkillsDataSchema.safeParse(raw);
    if (!validated.success) {
      return null;
    }
    const data = raw as CachedSkillsData;
    if (Date.now() - data.timestamp > CACHE_TTL_MS) {
      return null;
    }
    return data.skills.map(skill => ({
      ...skill,
      source,
    }));
  }, null);
}

function writeCachedSkills(
  source: MarketplaceSource,
  skills: MarketplaceSkill[]
): void {
  trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    const skillsToCache = skills.map(({ source: _, ...rest }) => rest);
    const data: CachedSkillsData = {
      timestamp: Date.now(),
      skills: skillsToCache as MarketplaceSkill[],
    };
    writeFileContent(cacheFile, JSON.stringify(data, null, 2));
    return true;
  }, false);
}

interface GitHubTreeItem {
  path: string;
  mode: string;
  type: 'blob' | 'tree';
  sha: string;
  size?: number;
  url: string;
}

interface GitHubTreeResponse {
  sha: string;
  url: string;
  tree: GitHubTreeItem[];
  truncated: boolean;
}

function formatSkillName(name: string): string {
  const acronyms = ['PR', 'API', 'UI', 'CLI', 'MCP', 'AI'];

  return name
    .replace(/\.md$/i, '')
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase())
    .replace(new RegExp(`\\b(${acronyms.join('|')})\\b`, 'gi'), match =>
      match.toUpperCase()
    );
}

function fetchLocalSkills(source: MarketplaceSource): MarketplaceSkill[] {
  try {
    const skillsSourcePath = getSkillsSourcePath();
    const availableSkills = getAvailableSkills();
    const skills: MarketplaceSkill[] = [];

    for (const skillFolder of availableSkills) {
      const skillPath = join(skillsSourcePath, skillFolder);
      const skillMdPath = join(skillPath, 'SKILL.md');

      if (fileExists(skillMdPath)) {
        const content = readFileContent(skillMdPath);
        if (content) {
          const meta = parseSkillFrontmatter(content);
          skills.push({
            name: skillFolder,
            displayName: formatSkillName(skillFolder),
            description:
              meta?.description ||
              extractFirstParagraph(content) ||
              'No description',
            category: meta?.category || 'Official',
            path: skillFolder,
            source,
          });
        }
      }
    }

    return skills;
  } catch {
    return [];
  }
}

function installLocalSkill(
  skill: MarketplaceSkill,
  destDir: string
): { success: boolean; error?: string } {
  try {
    const skillsSourcePath = getSkillsSourcePath();
    const sourcePath = resolveSkillDestination(skillsSourcePath, skill.name);
    const destPath = resolveSkillDestination(destDir, skill.name);

    if (!sourcePath || !destPath) {
      return { success: false, error: 'Invalid skill name' };
    }

    if (!dirExists(sourcePath)) {
      return { success: false, error: 'Skill not found in bundled source' };
    }

    const installResult = installSkillToDestination({
      sourcePath,
      destinationPath: destPath,
      mode: 'copy',
      force: true,
    });

    if (installResult !== 'installed') {
      return { success: false, error: 'Failed to copy bundled skill' };
    }

    return { success: true };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

export async function fetchMarketplaceTree(
  source: MarketplaceSource
): Promise<GitHubTreeItem[]> {
  const apiUrl = `https://api.github.com/repos/${source.owner}/${source.repo}/git/trees/${source.branch}?recursive=1`;

  const response = await fetch(apiUrl, {
    headers: {
      Accept: 'application/vnd.github.v3+json',
      'User-Agent': 'octocode',
    },
  });

  if (!response.ok) {
    if (response.status === 403) {
      throw new Error('GitHub API rate limit exceeded. Try again later.');
    }
    throw new Error(`Failed to fetch marketplace: ${response.statusText}`);
  }

  const data = (await response.json()) as GitHubTreeResponse;

  if (data.truncated) {
    console.warn(
      `[octocode] GitHub tree response was truncated for ${source.owner}/${source.repo}. Some skills may be missing.`
    );
  }

  return data.tree;
}

const MAX_CONTENT_SIZE_BYTES = 1024 * 1024;
const MAX_SKILL_FILES = 500;

export async function fetchRawContent(
  source: MarketplaceSource,
  path: string
): Promise<string> {
  const rawUrl = `https://raw.githubusercontent.com/${source.owner}/${source.repo}/${source.branch}/${path}`;

  const response = await fetch(rawUrl, {
    headers: {
      'User-Agent': 'octocode',
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch content: ${response.statusText}`);
  }

  const contentLength = response.headers?.get?.('Content-Length');
  const contentLengthBytes = contentLength ? Number(contentLength) : NaN;
  if (
    Number.isFinite(contentLengthBytes) &&
    contentLengthBytes > MAX_CONTENT_SIZE_BYTES
  ) {
    throw new Error(
      `Content too large: ${contentLengthBytes} bytes exceeds ${MAX_CONTENT_SIZE_BYTES} byte limit`
    );
  }

  const content = await response.text();
  const contentSizeBytes = Buffer.byteLength(content, 'utf8');

  if (contentSizeBytes > MAX_CONTENT_SIZE_BYTES) {
    throw new Error(
      `Content too large: ${contentSizeBytes} bytes exceeds ${MAX_CONTENT_SIZE_BYTES} byte limit`
    );
  }

  return content;
}

export async function fetchMarketplaceSkills(
  source: MarketplaceSource,
  options: { skipCache?: boolean } = {}
): Promise<MarketplaceSkill[]> {
  if (isLocalSource(source)) {
    return fetchLocalSkills(source);
  }

  if (!options.skipCache) {
    const cached = readCachedSkills(source);
    if (cached) {
      return cached;
    }
  }

  const tree = await fetchMarketplaceTree(source);
  const skills: MarketplaceSkill[] = [];

  if (source.skillPattern === 'flat-md') {
    const prefix = source.skillsPath ? `${source.skillsPath}/` : '';
    const mdFiles = tree.filter(
      item =>
        item.type === 'blob' &&
        item.path.startsWith(prefix) &&
        item.path.endsWith('.md') &&
        !item.path.includes('/') === (prefix === '') &&
        (prefix === '' || item.path.split('/').length === 2)
    );

    const filesToFetch = mdFiles.slice(0, 100);

    const results = await Promise.all(
      filesToFetch.map(async file => {
        try {
          const content = await fetchRawContent(source, file.path);
          const meta = parseSkillFrontmatter(content);
          const filename = file.path.split('/').pop() || file.path;
          return {
            name: filename.replace(/\.md$/i, ''),
            displayName: formatSkillName(filename),
            description: meta?.description || 'No description available',
            category: meta?.category,
            path: file.path,
            source,
          } as MarketplaceSkill;
        } catch {
          return null;
        }
      })
    );
    skills.push(...results.filter((s): s is MarketplaceSkill => s !== null));
  } else {
    const prefix = source.skillsPath ? `${source.skillsPath}/` : '';

    const skillDirs = tree.filter(
      item =>
        item.type === 'tree' &&
        (prefix === '' || item.path.startsWith(prefix)) &&
        !item.path.includes('.') &&
        !item.path.startsWith('.')
    );

    const results = await Promise.all(
      skillDirs.slice(0, 50).map(async dir => {
        const skillMdPath = `${dir.path}/SKILL.md`;
        const readmePath = `${dir.path}/README.md`;

        const hasSkillMd = tree.some(
          item => item.path === skillMdPath && item.type === 'blob'
        );
        const hasReadme = tree.some(
          item => item.path === readmePath && item.type === 'blob'
        );

        const filePath = hasSkillMd
          ? skillMdPath
          : hasReadme
            ? readmePath
            : null;

        if (!filePath) return null;

        try {
          const content = await fetchRawContent(source, filePath);
          const meta = parseSkillFrontmatter(content);
          const folderName = dir.path.split('/').pop() || dir.path;
          return {
            name: folderName,
            displayName: formatSkillName(folderName),
            description:
              meta?.description ||
              extractFirstParagraph(content) ||
              'No description',
            category: meta?.category,
            path: dir.path,
            source,
          } as MarketplaceSkill;
        } catch {
          return null;
        }
      })
    );
    skills.push(...results.filter((s): s is MarketplaceSkill => s !== null));
  }

  writeCachedSkills(source, skills);

  return skills;
}

function extractFirstParagraph(content: string): string | null {
  const withoutFrontmatter = content.replace(/^---[\s\S]*?---\s*/, '');

  const lines = withoutFrontmatter.split('\n');
  let paragraph = '';

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      if (paragraph) break;
      continue;
    }
    if (trimmed.startsWith('#')) continue;
    paragraph += (paragraph ? ' ' : '') + trimmed;
  }

  return paragraph ? paragraph.slice(0, 200) : null;
}

export async function installMarketplaceSkill(
  skill: MarketplaceSkill,
  destDir: string
): Promise<{ success: boolean; error?: string }> {
  try {
    const source = skill.source;

    if (isLocalSource(source)) {
      return installLocalSkill(skill, destDir);
    }

    const tree = await fetchMarketplaceTree(source);

    const skillDestDir = resolveSkillDestination(destDir, skill.name);
    if (!skillDestDir) {
      return { success: false, error: 'Invalid skill name' };
    }

    if (source.skillPattern === 'flat-md') {
      const content = await fetchRawContent(source, skill.path);
      const skillMdPath = join(skillDestDir, 'SKILL.md');
      if (!isPathInside(skillDestDir, skillMdPath)) {
        throw new Error('Invalid skill destination path');
      }
      prepareSkillDestination(skillDestDir);
      if (!writeFileContent(skillMdPath, content)) {
        throw new Error('Failed to write skill file');
      }
    } else {
      const prefix = skill.path ? `${skill.path}/` : '';
      const files = tree.filter(
        item => item.type === 'blob' && item.path.startsWith(prefix)
      );

      if (files.length === 0) {
        throw new Error(`Skill folder not found: ${skill.path || '/'}`);
      }

      if (files.length > MAX_SKILL_FILES) {
        throw new Error(
          `Skill has too many files: ${files.length} exceeds ${MAX_SKILL_FILES}`
        );
      }

      const plannedFiles = files.map(file => {
        const relativePath = file.path.slice(prefix.length);
        if (!relativePath || isAbsolute(relativePath)) {
          throw new Error('Invalid skill file path');
        }
        const destPath = join(skillDestDir, relativePath);
        if (!isPathInside(skillDestDir, destPath)) {
          throw new Error('Invalid skill file path traversal');
        }
        return { relativePath, destPath, sourcePath: file.path };
      });

      const fetchedFiles = await Promise.all(
        plannedFiles.map(async file => ({
          ...file,
          content: await fetchRawContent(source, file.sourcePath),
        }))
      );

      prepareSkillDestination(skillDestDir);

      for (const file of fetchedFiles) {
        const destSubDir = join(
          skillDestDir,
          file.relativePath.split('/').slice(0, -1).join('/')
        );
        if (destSubDir !== skillDestDir && !dirExists(destSubDir)) {
          mkdirSync(destSubDir, { recursive: true, mode: 0o700 });
        }
        if (!writeFileContent(file.destPath, file.content)) {
          throw new Error(`Failed to write skill file: ${file.relativePath}`);
        }
      }
    }

    return { success: true };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}

function prepareSkillDestination(skillDestDir: string): void {
  if (dirExists(skillDestDir)) {
    rmSync(skillDestDir, { recursive: true, force: true });
  }

  if (!dirExists(skillDestDir)) {
    mkdirSync(skillDestDir, { recursive: true, mode: 0o700 });
  }
}

export function searchSkills(
  skills: MarketplaceSkill[],
  query: string
): MarketplaceSkill[] {
  const lowerQuery = query.toLowerCase();
  return skills.filter(
    skill =>
      skill.name.toLowerCase().includes(lowerQuery) ||
      skill.displayName.toLowerCase().includes(lowerQuery) ||
      skill.description.toLowerCase().includes(lowerQuery) ||
      skill.category?.toLowerCase().includes(lowerQuery)
  );
}

export function groupSkillsByCategory(
  skills: MarketplaceSkill[]
): Map<string, MarketplaceSkill[]> {
  const grouped = new Map<string, MarketplaceSkill[]>();

  for (const skill of skills) {
    const category = skill.category || 'Other';
    if (!grouped.has(category)) {
      grouped.set(category, []);
    }
    grouped.get(category)!.push(skill);
  }

  return grouped;
}

export function clearSkillsCache(): void {
  trySafe(() => {
    const cacheDir = getCacheDir();
    if (dirExists(cacheDir)) {
      const files = readdirSync(cacheDir);
      for (const file of files) {
        if (file.endsWith('.json')) {
          unlinkSync(join(cacheDir, file));
        }
      }
    }
    return true;
  }, false);
}

export function clearSourceCache(source: MarketplaceSource): void {
  trySafe(() => {
    const cacheFile = getCacheFilePath(source);
    if (fileExists(cacheFile)) {
      unlinkSync(cacheFile);
    }
    return true;
  }, false);
}

export function getCacheInfo(source: MarketplaceSource): {
  isCached: boolean;
  age: number | null;
  expiresIn: number | null;
} {
  return trySafe(
    () => {
      const cacheFile = getCacheFilePath(source);
      if (!fileExists(cacheFile)) {
        return { isCached: false, age: null, expiresIn: null };
      }
      const stats = statSync(cacheFile);
      const age = Date.now() - stats.mtimeMs;
      const expiresIn = Math.max(0, CACHE_TTL_MS - age);
      return {
        isCached: age < CACHE_TTL_MS,
        age,
        expiresIn: age < CACHE_TTL_MS ? expiresIn : null,
      };
    },
    { isCached: false, age: null, expiresIn: null }
  );
}

export function getSkillsCacheDir(): string {
  return getCacheDir();
}

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

export interface SkillsShResult {
  id: string;
  skillId: string;
  name: string;
  installs: number;

  source: string;
}

export interface SkillsShSearchResponse {
  results: SkillsShResult[];
  count: number;
}

const SKILLS_SH_API = 'https://www.skills.sh/api/search';

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
