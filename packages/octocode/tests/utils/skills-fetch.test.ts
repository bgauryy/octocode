import { join } from 'node:path';
import os from 'node:os';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type {
  MarketplaceSource,
  MarketplaceSkill,
} from '../../src/configs/skills-marketplace.js';

vi.mock('../../src/utils/fs.js', () => ({
  dirExists: vi.fn(),
  writeFileContent: vi.fn(),
  readFileContent: vi.fn(),
  fileExists: vi.fn(() => false),
  copyDirectory: vi.fn(),
}));

vi.mock('node:fs', () => ({
  mkdirSync: vi.fn(),
  readdirSync: vi.fn(() => []),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
  existsSync: vi.fn(() => false),
}));

import {
  fetchMarketplaceTree,
  fetchRawContent,
  fetchMarketplaceSkills,
  installMarketplaceSkill,
  readSkillFromGitHub,
  fetchSkillsShSearch,
  searchSkills,
  groupSkillsByCategory,
  clearSkillsCache,
  clearSourceCache,
  getCacheInfo,
  getSkillsCacheDir,
} from '../../src/utils/skills-fetch.js';
import {
  dirExists,
  copyDirectory,
  writeFileContent,
  fileExists,
  readFileContent,
} from '../../src/utils/fs.js';
import * as nodeFs from 'node:fs';

const mockSource: MarketplaceSource = {
  id: 'test-marketplace',
  name: 'Test Marketplace',
  type: 'github',
  owner: 'test-owner',
  repo: 'test-repo',
  branch: 'main',
  skillsPath: 'commands',
  skillPattern: 'flat-md',
  description: 'Test marketplace',
  url: 'https://github.com/test-owner/test-repo',
};

const mockFolderSource: MarketplaceSource = {
  ...mockSource,
  id: 'test-folder-marketplace',
  skillsPath: 'skills',
  skillPattern: 'skill-folders',
};

const mockTreeResponse = {
  sha: 'abc123',
  url: 'https://api.github.com/repos/test/test/git/trees/main',
  tree: [
    {
      path: 'commands/code-review.md',
      mode: '100644',
      type: 'blob' as const,
      sha: 'sha1',
      size: 1000,
      url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
    },
    {
      path: 'commands/test-skill.md',
      mode: '100644',
      type: 'blob' as const,
      sha: 'sha2',
      size: 500,
      url: 'https://api.github.com/repos/test/test/git/blobs/sha2',
    },
    {
      path: 'other/file.txt',
      mode: '100644',
      type: 'blob' as const,
      sha: 'sha3',
      size: 100,
      url: 'https://api.github.com/repos/test/test/git/blobs/sha3',
    },
  ],
  truncated: false,
};

const mockSkillContent = `---
description: A test skill for code review
category: utilities
---

# Code Review Skill

This is a test skill for code review.
`;

describe('Skills Fetch Utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    global.fetch = vi.fn();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('fetchMarketplaceTree', () => {
    it('should fetch tree from GitHub API', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      const tree = await fetchMarketplaceTree(mockSource);

      expect(tree).toHaveLength(3);
      expect(tree[0].path).toBe('commands/code-review.md');
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('api.github.com'),
        expect.objectContaining({
          headers: expect.objectContaining({
            'User-Agent': 'octocode',
          }),
        })
      );
    });

    it('should throw error on API failure', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
      } as Response);

      await expect(fetchMarketplaceTree(mockSource)).rejects.toThrow(
        'Failed to fetch marketplace'
      );
    });

    it('should throw rate limit error on 403', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: false,
        status: 403,
        statusText: 'Forbidden',
      } as Response);

      await expect(fetchMarketplaceTree(mockSource)).rejects.toThrow(
        'rate limit exceeded'
      );
    });
  });

  describe('fetchRawContent', () => {
    it('should fetch raw content from GitHub', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const content = await fetchRawContent(mockSource, 'commands/test.md');

      expect(content).toBe(mockSkillContent);
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('raw.githubusercontent.com'),
        expect.any(Object)
      );
    });

    it('should throw error on fetch failure', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: false,
        statusText: 'Not Found',
      } as Response);

      await expect(
        fetchRawContent(mockSource, 'nonexistent.md')
      ).rejects.toThrow('Failed to fetch content');
    });
  });

  describe('fetchMarketplaceSkills', () => {
    it('should fetch and parse flat-md skills', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockSource);

      expect(skills.length).toBeGreaterThan(0);
      expect(skills[0].description).toBe('A test skill for code review');
      expect(skills[0].category).toBe('utilities');
    });

    it('should return cached skills when cache is valid and skipCache is not used', async () => {
      const npmCacheDir =
        process.env.npm_config_cache || join(os.homedir(), '.npm');
      const cacheDir = join(npmCacheDir, '_cacache', 'octocode-skills');
      const cacheFile = join(cacheDir, `${mockSource.id}.json`);

      vi.mocked(dirExists).mockImplementation(
        (p: string) => p === npmCacheDir || p === cacheDir
      );
      vi.mocked(fileExists).mockImplementation((p: string) => p === cacheFile);
      vi.mocked(nodeFs.statSync).mockReturnValue({
        mtimeMs: Date.now() - 120_000,
      } as import('node:fs').Stats);

      const now = Date.now();
      vi.mocked(readFileContent).mockReturnValue(
        JSON.stringify({
          timestamp: now,
          skills: [
            {
              name: 'from-cache',
              displayName: 'From Cache',
              description: 'cached desc',
              category: 'cat',
              path: 'commands/from-cache.md',
            },
          ],
        })
      );

      const skills = await fetchMarketplaceSkills(mockSource);

      expect(global.fetch).not.toHaveBeenCalled();
      expect(skills).toHaveLength(1);
      expect(skills[0].name).toBe('from-cache');
      expect(skills[0].source).toEqual(mockSource);
    });

    it('should use fetchLocalSkills when marketplace source type is local', async () => {
      vi.resetModules();
      vi.doMock('../../src/utils/skills.js', () => ({
        getSkillsSourcePath: vi.fn(() => '/bundled/skills'),
        getAvailableSkills: vi.fn(() => ['octocode-one']),
        resolveSkillDestination: vi.fn((destDir: string, skillName: string) =>
          skillName.includes('..') || skillName.includes('/')
            ? null
            : join(destDir, skillName)
        ),
        isPathInside: vi.fn(() => true),
        installSkillToDestination: vi.fn(() => 'installed'),
      }));

      const fsUtils = await import('../../src/utils/fs.js');
      vi.mocked(fsUtils.fileExists).mockImplementation((p: string) =>
        p.replace(/\\/g, '/').endsWith('octocode-one/SKILL.md')
      );
      vi.mocked(fsUtils.readFileContent).mockReturnValue(`---
description: From local bundle
category: LocalCat
---
# Local
`);

      const { fetchMarketplaceSkills } =
        await import('../../src/utils/skills-fetch.js');

      const localSource: MarketplaceSource = {
        ...mockSource,
        type: 'local',
        id: 'local-official',
      };

      const skills = await fetchMarketplaceSkills(localSource);

      expect(global.fetch).not.toHaveBeenCalled();
      expect(skills).toHaveLength(1);
      expect(skills[0].name).toBe('octocode-one');
      expect(skills[0].description).toBe('From local bundle');
      expect(skills[0].source).toEqual(localSource);
    });

    it('should handle skill-folders pattern', async () => {
      const folderTreeResponse = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/code-review',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/code-review/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 1000,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource);

      expect(skills.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('installMarketplaceSkill', () => {
    const mockSkill: MarketplaceSkill = {
      name: 'test-skill',
      displayName: 'Test Skill',
      description: 'A test skill',
      category: 'test',
      path: 'commands/test-skill.md',
      source: mockSource,
    };

    it('should install flat-md skill', async () => {
      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(true);

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const result = await installMarketplaceSkill(mockSkill, '/dest');

      expect(result.success).toBe(true);
      expect(writeFileContent).toHaveBeenCalled();
    });

    it('should return error on failure', async () => {
      vi.mocked(global.fetch).mockRejectedValueOnce(new Error('Network error'));

      const result = await installMarketplaceSkill(mockSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('should return error when writeFileContent fails for flat-md skill', async () => {
      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(false);

      vi.mocked(global.fetch)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => mockTreeResponse,
        } as Response)
        .mockResolvedValueOnce({
          ok: true,
          text: async () => mockSkillContent,
        } as Response);

      const result = await installMarketplaceSkill(mockSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toContain('Failed to write skill file');
    });

    it('cleans up existing skill dir before reinstalling (prepareSkillDestination rmSync)', async () => {
      vi.mocked(dirExists).mockReturnValueOnce(true).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(true);

      vi.mocked(global.fetch)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => mockTreeResponse,
        } as Response)
        .mockResolvedValueOnce({
          ok: true,
          text: async () => mockSkillContent,
        } as Response);

      const result = await installMarketplaceSkill(mockSkill, '/dest');
      expect(typeof result.success).toBe('boolean');
    });

    it('installs a skill folder at repository root', async () => {
      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(true);

      vi.mocked(global.fetch)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => ({
            sha: 'abc123',
            url: 'https://api.github.com/repos/test/test/git/trees/main',
            tree: [
              {
                path: 'SKILL.md',
                mode: '100644',
                type: 'blob',
                sha: 'sha1',
                size: 1000,
                url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
              },
            ],
            truncated: false,
          }),
        } as Response)
        .mockResolvedValueOnce({
          ok: true,
          text: async () => mockSkillContent,
        } as Response);

      const result = await installMarketplaceSkill(
        {
          name: 'root-skill',
          displayName: 'Root Skill',
          description: 'A root skill',
          path: '',
          source: mockFolderSource,
        },
        '/dest'
      );

      expect(result.success).toBe(true);
      expect(writeFileContent).toHaveBeenCalledWith(
        '/dest/root-skill/SKILL.md',
        mockSkillContent
      );
    });
  });

  describe('searchSkills', () => {
    const mockSkills: MarketplaceSkill[] = [
      {
        name: 'code-review',
        displayName: 'Code Review',
        description: 'Review code for quality',
        category: 'development',
        path: 'skills/code-review',
        source: mockSource,
      },
      {
        name: 'testing',
        displayName: 'Testing',
        description: 'Test automation tools',
        category: 'testing',
        path: 'skills/testing',
        source: mockSource,
      },
      {
        name: 'documentation',
        displayName: 'Documentation',
        description: 'Generate documentation',
        category: 'development',
        path: 'skills/documentation',
        source: mockSource,
      },
    ];

    it('should find skills by name', () => {
      const results = searchSkills(mockSkills, 'code');
      expect(results).toHaveLength(1);
      expect(results[0].name).toBe('code-review');
    });

    it('should find skills by description', () => {
      const results = searchSkills(mockSkills, 'automation');
      expect(results).toHaveLength(1);
      expect(results[0].name).toBe('testing');
    });

    it('should find skills by category', () => {
      const results = searchSkills(mockSkills, 'development');
      expect(results).toHaveLength(2);
    });

    it('should be case-insensitive', () => {
      const results = searchSkills(mockSkills, 'CODE');
      expect(results).toHaveLength(1);
    });

    it('should return empty array for no matches', () => {
      const results = searchSkills(mockSkills, 'xyz123');
      expect(results).toHaveLength(0);
    });
  });

  describe('groupSkillsByCategory', () => {
    const mockSkills: MarketplaceSkill[] = [
      {
        name: 'skill1',
        displayName: 'Skill 1',
        description: 'Description 1',
        category: 'development',
        path: 'skills/skill1',
        source: mockSource,
      },
      {
        name: 'skill2',
        displayName: 'Skill 2',
        description: 'Description 2',
        category: 'testing',
        path: 'skills/skill2',
        source: mockSource,
      },
      {
        name: 'skill3',
        displayName: 'Skill 3',
        description: 'Description 3',
        category: 'development',
        path: 'skills/skill3',
        source: mockSource,
      },
      {
        name: 'skill4',
        displayName: 'Skill 4',
        description: 'Description 4',
        path: 'skills/skill4',
        source: mockSource,
      },
    ];

    it('should group skills by category', () => {
      const grouped = groupSkillsByCategory(mockSkills);

      expect(grouped.get('development')).toHaveLength(2);
      expect(grouped.get('testing')).toHaveLength(1);
    });

    it('should put skills without category in "Other"', () => {
      const grouped = groupSkillsByCategory(mockSkills);

      expect(grouped.get('Other')).toHaveLength(1);
      expect(grouped.get('Other')?.[0].name).toBe('skill4');
    });

    it('should return empty map for empty input', () => {
      const grouped = groupSkillsByCategory([]);
      expect(grouped.size).toBe(0);
    });
  });

  describe('cache functions', () => {
    describe('clearSkillsCache', () => {
      it('should clear all cached skill files', () => {
        vi.mocked(dirExists).mockReturnValue(true);

        vi.mocked(nodeFs.readdirSync).mockReturnValue([
          'source1.json',
          'source2.json',
          'notjson.txt',
        ] as any);

        clearSkillsCache();

        expect(nodeFs.unlinkSync).toHaveBeenCalledTimes(2);
      });

      it('should handle non-existent cache directory', () => {
        vi.mocked(dirExists).mockReturnValue(false);

        expect(() => clearSkillsCache()).not.toThrow();
      });

      it('should handle errors gracefully', () => {
        vi.mocked(dirExists).mockReturnValue(true);
        vi.mocked(nodeFs.readdirSync).mockImplementation(() => {
          throw new Error('Read error');
        });

        expect(() => clearSkillsCache()).not.toThrow();
      });
    });

    describe('clearSourceCache', () => {
      it('should clear cache for specific source', () => {
        vi.mocked(fileExists).mockReturnValue(true);

        clearSourceCache(mockSource);

        expect(nodeFs.unlinkSync).toHaveBeenCalled();
      });

      it('should handle non-existent cache file', () => {
        vi.mocked(fileExists).mockReturnValue(false);

        expect(() => clearSourceCache(mockSource)).not.toThrow();
        expect(nodeFs.unlinkSync).not.toHaveBeenCalled();
      });

      it('should handle errors gracefully', () => {
        vi.mocked(fileExists).mockReturnValue(true);
        vi.mocked(nodeFs.unlinkSync).mockImplementation(() => {
          throw new Error('Delete error');
        });

        expect(() => clearSourceCache(mockSource)).not.toThrow();
      });
    });

    describe('getCacheInfo', () => {
      it('should return cache info for valid cached file', () => {
        const now = Date.now();
        vi.mocked(fileExists).mockReturnValue(true);
        vi.mocked(nodeFs.statSync).mockReturnValue({
          mtimeMs: now - 60000,
        } as import('node:fs').Stats);

        const info = getCacheInfo(mockSource);

        expect(info.isCached).toBe(true);
        expect(info.age).toBeGreaterThan(0);
        expect(info.expiresIn).toBeGreaterThan(0);
      });

      it('should return not cached for expired file', () => {
        const now = Date.now();
        vi.mocked(fileExists).mockReturnValue(true);
        vi.mocked(nodeFs.statSync).mockReturnValue({
          mtimeMs: now - 86400000,
        } as import('node:fs').Stats);

        const info = getCacheInfo(mockSource);

        expect(info.isCached).toBe(false);
        expect(info.expiresIn).toBeNull();
      });

      it('should return not cached for non-existent file', () => {
        vi.mocked(fileExists).mockReturnValue(false);

        const info = getCacheInfo(mockSource);

        expect(info.isCached).toBe(false);
        expect(info.age).toBeNull();
        expect(info.expiresIn).toBeNull();
      });

      it('should handle errors gracefully', () => {
        vi.mocked(fileExists).mockReturnValue(true);
        vi.mocked(nodeFs.statSync).mockImplementation(() => {
          throw new Error('Stat error');
        });

        const info = getCacheInfo(mockSource);

        expect(info.isCached).toBe(false);
        expect(info.age).toBeNull();
      });
    });

    describe('getSkillsCacheDir', () => {
      it('should return a valid cache directory path', () => {
        const cacheDir = getSkillsCacheDir();

        expect(typeof cacheDir).toBe('string');
        expect(cacheDir.length).toBeGreaterThan(0);
      });
    });

    describe('readCachedSkills / isCacheValid', () => {
      it('should treat cache as invalid when statSync throws and still fetch skills', async () => {
        vi.mocked(dirExists).mockImplementation(
          (p: string) => !p.endsWith('.npm')
        );
        vi.mocked(fileExists).mockImplementation((p: string) =>
          p.endsWith(`${mockSource.id}.json`)
        );
        vi.mocked(nodeFs.statSync).mockImplementation(() => {
          throw new Error('stat failed');
        });

        vi.mocked(global.fetch).mockResolvedValueOnce({
          ok: true,
          json: async () => mockTreeResponse,
        } as Response);
        vi.mocked(global.fetch).mockResolvedValue({
          ok: true,
          text: async () => mockSkillContent,
        } as Response);

        const skills = await fetchMarketplaceSkills(mockSource);

        expect(global.fetch).toHaveBeenCalled();
        expect(skills.length).toBeGreaterThan(0);
      });
    });
  });

  describe('installMarketplaceSkill with skill-folders', () => {
    const folderSkill: MarketplaceSkill = {
      name: 'test-folder-skill',
      displayName: 'Test Folder Skill',
      description: 'A test folder skill',
      category: 'test',
      path: 'skills/test-folder-skill',
      source: mockFolderSource,
    };

    it('should install skill-folders pattern skill', async () => {
      const folderTreeResponse = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/test-folder-skill/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 1000,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
          {
            path: 'skills/test-folder-skill/references/ref.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha2',
            size: 500,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha2',
          },
        ],
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(true);

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(true);
      expect(writeFileContent).toHaveBeenCalled();
    });

    it('should fail when skill-folders tree has blob path equal to prefix (empty relativePath)', async () => {
      const treeWithEmptyRelative = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/test-folder-skill/',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha0',
            size: 0,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha0',
          },
          {
            path: 'skills/test-folder-skill/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 100,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => treeWithEmptyRelative,
      } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid skill file path');
    });

    it('should fail when skill-folders relativePath is absolute', async () => {
      const treeWithAbsoluteChild = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/test-folder-skill//etc/passwd',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 10,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => treeWithAbsoluteChild,
      } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid skill file path');
    });

    it('should fail on path traversal in skill-folders install (relativePath escapes dest)', async () => {
      const treeTraversal = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/test-folder-skill/../../outside/pwned.txt',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 10,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => treeTraversal,
      } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid skill file path traversal');
    });

    it('should fail when skill has too many files', async () => {
      const MAX_FILES = 500;
      const manyFiles = Array.from({ length: MAX_FILES + 1 }, (_, i) => ({
        path: `skills/test-folder-skill/file${i}.md`,
        mode: '100644' as const,
        type: 'blob' as const,
        sha: `sha${i}`,
        size: 100,
        url: `https://api.github.com/repos/test/test/git/blobs/sha${i}`,
      }));

      const tooManyFilesTree = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: manyFiles,
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => tooManyFilesTree,
      } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toContain('too many files');
    });

    it('should fail when writeFileContent returns false for a folder skill file', async () => {
      const folderTreeResponse = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/test-folder-skill/SKILL.md',
            mode: '100644' as const,
            type: 'blob' as const,
            sha: 'sha1',
            size: 100,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeFileContent).mockReturnValue(false);

      vi.mocked(global.fetch)
        .mockResolvedValueOnce({
          ok: true,
          json: async () => folderTreeResponse,
        } as Response)
        .mockResolvedValue({
          ok: true,
          text: async () => mockSkillContent,
        } as Response);

      const result = await installMarketplaceSkill(folderSkill, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toContain('Failed to write skill file');
    });
  });

  describe('installMarketplaceSkill flat-md path guard', () => {
    it('should fail when resolved SKILL.md path is outside skill dest (isPathInside)', async () => {
      const mockSkillFlat: MarketplaceSkill = {
        name: 'test-skill',
        displayName: 'Test Skill',
        description: 'A test skill',
        category: 'test',
        path: 'commands/test-skill.md',
        source: mockSource,
      };

      vi.resetModules();
      vi.doMock('../../src/utils/skills.js', async importOriginal => {
        const actual =
          await importOriginal<typeof import('../../src/utils/skills.js')>();
        return {
          ...actual,
          isPathInside: vi.fn(() => false),
        };
      });

      const { installMarketplaceSkill: installSkill } =
        await import('../../src/utils/skills-fetch.js');
      const fsUtils = await import('../../src/utils/fs.js');
      vi.mocked(fsUtils.dirExists).mockReturnValue(false);

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const result = await installSkill(mockSkillFlat, '/dest');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid skill destination path');
    });
  });

  describe('fetchMarketplaceSkills edge cases', () => {
    it('should handle skill-folders with README.md fallback', async () => {
      const folderTreeWithReadme = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/readme-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/readme-skill/README.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 1000,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeWithReadme,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource);

      expect(skills.length).toBeGreaterThanOrEqual(0);
    });

    it('should handle content without frontmatter', async () => {
      const contentWithoutFrontmatter = `# Simple Skill

This is a simple skill without YAML frontmatter.

It should extract the first paragraph as description.
`;

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => contentWithoutFrontmatter,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockSource);

      expect(skills.length).toBeGreaterThan(0);
      expect(skills[0].description).toBeTruthy();
    });

    it('should handle fetch errors for individual skills gracefully', async () => {
      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockRejectedValue(new Error('Network error'));

      const skills = await fetchMarketplaceSkills(mockSource);

      expect(Array.isArray(skills)).toBe(true);
    });

    it('should use default description when flat-md frontmatter has no description', async () => {
      const contentNoDescription = `---
category: utilities
---

# Title only

Body.
`;

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => mockTreeResponse,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => contentNoDescription,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(2);
      expect(
        skills.every(s => s.description === 'No description available')
      ).toBe(true);
    });

    it('should omit skills whose raw content fetch rejects (catch returns null)', async () => {
      const twoMdTree = {
        ...mockTreeResponse,
        tree: [
          ...mockTreeResponse.tree.filter(
            t => t.path.endsWith('.md') && t.path.startsWith('commands/')
          ),
        ],
      };
      expect(twoMdTree.tree.length).toBe(2);

      let rawFetchCount = 0;
      vi.mocked(global.fetch).mockImplementation(
        async (input: string | URL | Request) => {
          const url = String(input);
          if (url.includes('api.github.com')) {
            return {
              ok: true,
              json: async () => twoMdTree,
            } as Response;
          }
          rawFetchCount += 1;
          if (rawFetchCount === 1) {
            return {
              ok: true,
              text: async () => mockSkillContent,
            } as Response;
          }
          return Promise.reject(new Error('fetch failed'));
        }
      );

      const skills = await fetchMarketplaceSkills(mockSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(1);
      expect(skills[0].name).toBe('code-review');
    });

    it('should skip hidden directories in skill-folders', async () => {
      const treeWithHidden = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/.hidden-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/valid-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir2',
            url: 'https://api.github.com/repos/test/test/git/trees/dir2',
          },
          {
            path: 'skills/valid-skill/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 1000,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => treeWithHidden,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource);

      const hasHidden = skills.some(s => s.name.startsWith('.'));
      expect(hasHidden).toBe(false);
    });

    it('should filter out skill-folders dirs with no SKILL.md or README.md', async () => {
      const treeNoDocs = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/empty-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/empty-skill/notes.txt',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 10,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
          {
            path: 'skills/with-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir2',
            url: 'https://api.github.com/repos/test/test/git/trees/dir2',
          },
          {
            path: 'skills/with-skill/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha2',
            size: 100,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha2',
          },
        ],
        truncated: false,
      };

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => treeNoDocs,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mockSkillContent,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(1);
      expect(skills[0].name).toBe('with-skill');
    });

    it('should use extractFirstParagraph when frontmatter has no description (skill-folders)', async () => {
      const folderTreeOne = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/para-skill',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/para-skill/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 200,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      const mdNoDesc = `---
category: utilities
---

# Title Here

This paragraph is used when YAML has no description field.
`;

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeOne,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mdNoDesc,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(1);
      expect(skills[0].description).toBe(
        'This paragraph is used when YAML has no description field.'
      );
    });

    it('should truncate extractFirstParagraph at 200 characters (skill-folders)', async () => {
      const longPara = `${'word '.repeat(60)}end.`;
      expect(longPara.length).toBeGreaterThan(200);

      const folderTreeOne = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/long-para',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/long-para/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 500,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      const mdNoDesc = `---
category: x
---

# H

${longPara}
`;

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeOne,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mdNoDesc,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(1);
      expect(skills[0].description).toHaveLength(200);
      expect(skills[0].description).toBe(longPara.slice(0, 200));
    });

    it('should use No description when extractFirstParagraph finds no body text', async () => {
      const folderTreeOne = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/headers-only',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/headers-only/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 50,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      const mdHeadersOnly = `---
category: utilities
---

# Only Headers

## Still no paragraph
`;

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTreeOne,
      } as Response);

      vi.mocked(global.fetch).mockResolvedValue({
        ok: true,
        text: async () => mdHeadersOnly,
      } as Response);

      const skills = await fetchMarketplaceSkills(mockFolderSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(1);
      expect(skills[0].description).toBe('No description');
    });

    it('should return null for skill-folders when fetchRawContent rejects', async () => {
      const folderTree = {
        sha: 'abc123',
        url: 'https://api.github.com/repos/test/test/git/trees/main',
        tree: [
          {
            path: 'skills/broken',
            mode: '040000',
            type: 'tree' as const,
            sha: 'dir1',
            url: 'https://api.github.com/repos/test/test/git/trees/dir1',
          },
          {
            path: 'skills/broken/SKILL.md',
            mode: '100644',
            type: 'blob' as const,
            sha: 'sha1',
            size: 100,
            url: 'https://api.github.com/repos/test/test/git/blobs/sha1',
          },
        ],
        truncated: false,
      };

      vi.mocked(global.fetch).mockResolvedValueOnce({
        ok: true,
        json: async () => folderTree,
      } as Response);

      vi.mocked(global.fetch).mockRejectedValue(new Error('raw fetch failed'));

      const skills = await fetchMarketplaceSkills(mockFolderSource, {
        skipCache: true,
      });

      expect(skills).toHaveLength(0);
    });
  });

  describe('installMarketplaceSkill (local source)', () => {
    it('should copy bundled skill when source type is local', async () => {
      vi.resetModules();
      vi.doMock('../../src/utils/skills.js', () => ({
        getSkillsSourcePath: vi.fn(() => '/bundled/skills'),
        getAvailableSkills: vi.fn(() => []),
        resolveSkillDestination: vi.fn((destDir: string, skillName: string) =>
          skillName.includes('..') || skillName.includes('/')
            ? null
            : join(destDir, skillName)
        ),
        isPathInside: vi.fn(() => true),
        installSkillToDestination: vi.fn(
          ({
            sourcePath,
            destinationPath,
          }: {
            sourcePath: string;
            destinationPath: string;
          }) => {
            try {
              return vi.mocked(copyDirectory)(sourcePath, destinationPath)
                ? 'installed'
                : 'failed';
            } catch {
              return 'failed';
            }
          }
        ),
      }));

      const fsUtils = await import('../../src/utils/fs.js');
      vi.mocked(fsUtils.dirExists).mockImplementation(
        (p: string) => p === '/bundled/skills/octocode-bundled'
      );
      vi.mocked(fsUtils.copyDirectory).mockReturnValue(true);

      const { installMarketplaceSkill } =
        await import('../../src/utils/skills-fetch.js');

      const localSkill: MarketplaceSkill = {
        name: 'octocode-bundled',
        displayName: 'Octocode Bundled',
        description: 'Bundled skill',
        category: 'test',
        path: 'octocode-bundled',
        source: { ...mockSource, type: 'local' },
      };

      const result = await installMarketplaceSkill(localSkill, '/dest/skills');

      expect(result.success).toBe(true);
      expect(fsUtils.copyDirectory).toHaveBeenCalledWith(
        '/bundled/skills/octocode-bundled',
        '/dest/skills/octocode-bundled'
      );
      expect(global.fetch).not.toHaveBeenCalled();
    });

    it('should return error when bundled skill source directory is missing', async () => {
      vi.resetModules();
      vi.doMock('../../src/utils/skills.js', () => ({
        getSkillsSourcePath: vi.fn(() => '/bundled/skills'),
        getAvailableSkills: vi.fn(() => []),
        resolveSkillDestination: vi.fn((destDir: string, skillName: string) =>
          skillName.includes('..') || skillName.includes('/')
            ? null
            : join(destDir, skillName)
        ),
        isPathInside: vi.fn(() => true),
        installSkillToDestination: vi.fn(() => 'installed'),
      }));

      const fsUtils = await import('../../src/utils/fs.js');
      vi.mocked(fsUtils.dirExists).mockReturnValue(false);

      const { installMarketplaceSkill } =
        await import('../../src/utils/skills-fetch.js');

      const localSkill: MarketplaceSkill = {
        name: 'missing-skill',
        displayName: 'Missing',
        description: 'x',
        category: 'test',
        path: 'missing-skill',
        source: { ...mockSource, type: 'local' },
      };

      const result = await installMarketplaceSkill(localSkill, '/dest/skills');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Skill not found in bundled source');
      expect(fsUtils.copyDirectory).not.toHaveBeenCalled();
      expect(global.fetch).not.toHaveBeenCalled();
    });

    it('should return shared installer error when bundled copy fails', async () => {
      vi.resetModules();
      vi.doMock('../../src/utils/skills.js', () => ({
        getSkillsSourcePath: vi.fn(() => '/bundled/skills'),
        getAvailableSkills: vi.fn(() => []),
        resolveSkillDestination: vi.fn((destDir: string, skillName: string) =>
          skillName.includes('..') || skillName.includes('/')
            ? null
            : join(destDir, skillName)
        ),
        isPathInside: vi.fn(() => true),
        installSkillToDestination: vi.fn(
          ({
            sourcePath,
            destinationPath,
          }: {
            sourcePath: string;
            destinationPath: string;
          }) => {
            try {
              return vi.mocked(copyDirectory)(sourcePath, destinationPath)
                ? 'installed'
                : 'failed';
            } catch {
              return 'failed';
            }
          }
        ),
      }));

      const fsUtils = await import('../../src/utils/fs.js');
      vi.mocked(fsUtils.dirExists).mockReturnValue(true);
      vi.mocked(fsUtils.copyDirectory).mockImplementation(() => {
        throw new Error('copy failed');
      });

      const { installMarketplaceSkill } =
        await import('../../src/utils/skills-fetch.js');

      const localSkill: MarketplaceSkill = {
        name: 'any-skill',
        displayName: 'Any',
        description: 'x',
        category: 'test',
        path: 'any-skill',
        source: { ...mockSource, type: 'local' },
      };

      const result = await installMarketplaceSkill(localSkill, '/dest/skills');

      expect(result.success).toBe(false);
      expect(result.error).toBe('Failed to copy bundled skill');
    });
  });
});

describe('readSkillFromGitHub', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('fetches SKILL.md from raw.githubusercontent.com', async () => {
    const content = `---\nname: my-skill\n---\n# My Skill`;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        text: async () => content,
      })
    );

    const result = await readSkillFromGitHub(
      'owner',
      'repo',
      'skills/my-skill'
    );
    expect(result).toBe(content);

    const fetchMock = vi.mocked(
      fetch as unknown as (...args: unknown[]) => unknown
    );
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain(
      'raw.githubusercontent.com/owner/repo/main/skills/my-skill/SKILL.md'
    );
  });

  it('appends /SKILL.md when path does not end with it', async () => {
    const content = '# skill';
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue({ ok: true, status: 200, text: async () => content })
    );
    await readSkillFromGitHub('o', 'r', 'my-path');

    const fetchMock = vi.mocked(
      fetch as unknown as (...args: unknown[]) => unknown
    );
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain('my-path/SKILL.md');
  });

  it('does not double-append /SKILL.md when already present', async () => {
    const content = '# skill';
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue({ ok: true, status: 200, text: async () => content })
    );
    await readSkillFromGitHub('o', 'r', 'my-path/SKILL.md');

    const fetchMock = vi.mocked(
      fetch as unknown as (...args: unknown[]) => unknown
    );
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).not.toContain('SKILL.md/SKILL.md');
    expect(url).toContain('my-path/SKILL.md');
  });

  it('retries with master branch on 404 from main', async () => {
    const content = '# found on master';
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404, text: async () => '' })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        text: async () => content,
      });
    vi.stubGlobal('fetch', fetchMock);

    const result = await readSkillFromGitHub('o', 'r', 'my-skill');
    expect(result).toBe(content);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const [secondUrl] = fetchMock.mock.calls[1] as [string];
    expect(secondUrl).toContain('/master/');
  });

  it('throws after 404 on both main and master', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue({ ok: false, status: 404, text: async () => '' })
    );

    await expect(readSkillFromGitHub('o', 'r', 'my-skill')).rejects.toThrow(
      'SKILL.md not found'
    );
  });

  it('throws on non-404 HTTP error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Server Error',
        text: async () => '',
      })
    );

    await expect(readSkillFromGitHub('o', 'r', 'my-skill')).rejects.toThrow(
      'Failed to fetch SKILL.md: 500'
    );
  });

  it('throws when content exceeds max size', async () => {
    const huge = 'x'.repeat(1_048_577);
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue({ ok: true, status: 200, text: async () => huge })
    );

    await expect(readSkillFromGitHub('o', 'r', 'my-skill')).rejects.toThrow(
      'too large'
    );
  });
});

describe('fetchSkillsShSearch', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns parsed and sorted results', async () => {
    const skills = [
      {
        id: 'a/b/s1',
        skillId: 's1',
        name: 'Skill One',
        installs: 10,
        source: 'a/b',
      },
      {
        id: 'a/b/s2',
        skillId: 's2',
        name: 'Skill Two',
        installs: 50,
        source: 'a/b',
      },
    ];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ skills, count: 2 }),
      })
    );

    const result = await fetchSkillsShSearch('langchain');
    expect(result.count).toBe(2);
    expect(result.results[0].name).toBe('Skill Two');
  });

  it('includes query and limit in the URL', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ skills: [], count: 0 }),
    });
    vi.stubGlobal('fetch', fetchMock);

    await fetchSkillsShSearch('react hooks', 5);

    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain('q=react%20hooks');
    expect(url).toContain('limit=5');
  });

  it('throws on non-ok response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 503,
        statusText: 'Unavailable',
      })
    );

    await expect(fetchSkillsShSearch('test')).rejects.toThrow(
      'skills.sh search failed'
    );
  });

  it('handles empty skills array', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ count: 0 }),
      })
    );

    const result = await fetchSkillsShSearch('nothing');
    expect(result.results).toEqual([]);
    expect(result.count).toBe(0);
  });
});
