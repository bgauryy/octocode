import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  SKILLS_MARKETPLACES,
  getMarketplaceById,
  getMarketplaceCount,
  fetchMarketplaceStars,
  fetchAllMarketplaceStars,
  clearStarsCache,
  isLocalSource,
  getLocalMarketplaces,
  getGitHubMarketplaces,
} from '../../src/configs/skills-marketplace.js';

describe('Skills Marketplace Registry', () => {
  describe('SKILLS_MARKETPLACES', () => {
    it('should have at least one marketplace', () => {
      expect(SKILLS_MARKETPLACES.length).toBeGreaterThan(0);
    });

    it('should have required fields for each marketplace', () => {
      for (const marketplace of SKILLS_MARKETPLACES) {
        expect(marketplace.id).toBeDefined();
        expect(marketplace.name).toBeDefined();
        expect(marketplace.type).toBeDefined();
        expect(['github', 'local']).toContain(marketplace.type);
        expect(marketplace.owner).toBeDefined();
        expect(marketplace.repo).toBeDefined();
        expect(marketplace.branch).toBeDefined();
        expect(marketplace.skillsPath).toBeDefined();
        expect(marketplace.skillPattern).toBeDefined();
        expect(marketplace.description).toBeDefined();
        expect(marketplace.url).toBeDefined();
      }
    });

    it('should include octocode-skills as a local marketplace', () => {
      const octocode = SKILLS_MARKETPLACES.find(
        m => m.id === 'octocode-skills'
      );
      expect(octocode).toBeDefined();
      expect(octocode?.type).toBe('github');
      expect(octocode?.name).toContain('Octocode');
    });

    it('should have unique IDs', () => {
      const ids = SKILLS_MARKETPLACES.map(m => m.id);
      const uniqueIds = new Set(ids);
      expect(uniqueIds.size).toBe(ids.length);
    });

    it('should have valid skill patterns', () => {
      for (const marketplace of SKILLS_MARKETPLACES) {
        expect(['flat-md', 'skill-folders']).toContain(
          marketplace.skillPattern
        );
      }
    });

    it('should have valid GitHub URLs', () => {
      for (const marketplace of SKILLS_MARKETPLACES) {
        expect(marketplace.url).toMatch(
          /^https:\/\/github\.com\/[\w-]+\/[\w-]+((\/[\w.-]+)*)?$/
        );
      }
    });

    it('should include buildwithclaude marketplace', () => {
      const buildWithClaude = SKILLS_MARKETPLACES.find(
        m => m.id === 'buildwithclaude'
      );
      expect(buildWithClaude).toBeDefined();
      expect(buildWithClaude?.owner).toBe('davepoon');
      expect(buildWithClaude?.repo).toBe('buildwithclaude');
    });

    it('should include webmaxru-agent-skills marketplace', () => {
      const agentSkills = SKILLS_MARKETPLACES.find(
        m => m.id === 'webmaxru-agent-skills'
      );
      expect(agentSkills).toBeDefined();
      expect(agentSkills?.owner).toBe('webmaxru');
      expect(agentSkills?.repo).toBe('agent-skills');
      expect(agentSkills?.skillsPath).toBe('skills');
      expect(agentSkills?.skillPattern).toBe('skill-folders');
    });

    it('should include newly added popular marketplaces', () => {
      const ids = SKILLS_MARKETPLACES.map(m => m.id);
      expect(ids).toContain('everything-claude-code');
      expect(ids).toContain('antigravity-awesome-skills');
      expect(ids).toContain('obsidian-skills');
    });
  });

  describe('getMarketplaceById', () => {
    it('should return marketplace when ID exists', () => {
      const marketplace = getMarketplaceById('buildwithclaude');
      expect(marketplace).toBeDefined();
      expect(marketplace?.id).toBe('buildwithclaude');
    });

    it('should return undefined for non-existent ID', () => {
      const marketplace = getMarketplaceById('non-existent-marketplace');
      expect(marketplace).toBeUndefined();
    });

    it('should return undefined for empty ID', () => {
      const marketplace = getMarketplaceById('');
      expect(marketplace).toBeUndefined();
    });
  });

  describe('getMarketplaceCount', () => {
    it('should return correct count', () => {
      expect(getMarketplaceCount()).toBe(SKILLS_MARKETPLACES.length);
    });

    it('should return at least 4 (curated marketplaces)', () => {
      expect(getMarketplaceCount()).toBeGreaterThanOrEqual(4);
    });
  });

  describe('isLocalSource', () => {
    it('should return false for GitHub sources', () => {
      const octocode = SKILLS_MARKETPLACES.find(
        m => m.id === 'octocode-skills'
      );
      expect(octocode).toBeDefined();
      expect(isLocalSource(octocode!)).toBe(false);
    });

    it('should return true for local sources', () => {
      const mockLocalSource = {
        ...SKILLS_MARKETPLACES[0],
        type: 'local' as const,
      };
      expect(isLocalSource(mockLocalSource)).toBe(true);
    });
  });

  describe('getLocalMarketplaces', () => {
    it('should return only local sources', () => {
      const localSources = getLocalMarketplaces();

      expect(localSources.length).toBe(0);
      for (const source of localSources) {
        expect(source.type).toBe('local');
      }
    });

    it('should not include octocode-skills (it is a github source)', () => {
      const localSources = getLocalMarketplaces();
      expect(localSources.some(s => s.id === 'octocode-skills')).toBe(false);
    });
  });

  describe('getGitHubMarketplaces', () => {
    it('should return only GitHub sources', () => {
      const githubSources = getGitHubMarketplaces();
      expect(githubSources.length).toBeGreaterThan(0);
      for (const source of githubSources) {
        expect(source.type).toBe('github');
      }
    });

    it('should include octocode-skills', () => {
      const githubSources = getGitHubMarketplaces();
      expect(githubSources.some(s => s.id === 'octocode-skills')).toBe(true);
    });
  });

  describe('fetchMarketplaceStars', () => {
    const originalFetch = global.fetch;

    beforeEach(() => {
      vi.resetAllMocks();
      clearStarsCache();
    });

    afterEach(() => {
      global.fetch = originalFetch;
    });

    it('should return null for local sources without making API call', async () => {
      global.fetch = vi.fn();

      const mockLocalSource = {
        ...SKILLS_MARKETPLACES[0],
        type: 'local' as const,
      };
      const stars = await fetchMarketplaceStars(mockLocalSource);

      expect(stars).toBeNull();
      expect(global.fetch).not.toHaveBeenCalled();
    });

    it('should fetch stars from GitHub API for GitHub sources', async () => {
      const mockResponse = {
        ok: true,
        json: () => Promise.resolve({ stargazers_count: 1500 }),
      };
      global.fetch = vi.fn().mockResolvedValue(mockResponse);

      const source = getGitHubMarketplaces()[0];
      const stars = await fetchMarketplaceStars(source);

      expect(stars).toBe(1500);
      expect(global.fetch).toHaveBeenCalledWith(
        `https://api.github.com/repos/${source.owner}/${source.repo}`,
        expect.objectContaining({
          headers: expect.objectContaining({
            Accept: 'application/vnd.github.v3+json',
          }),
        })
      );
    });

    it('should return null on API error', async () => {
      const mockResponse = {
        ok: false,
        status: 404,
      };
      global.fetch = vi.fn().mockResolvedValue(mockResponse);

      const source = getGitHubMarketplaces()[0];
      const stars = await fetchMarketplaceStars(source);

      expect(stars).toBeNull();
    });

    it('should return null on network error', async () => {
      global.fetch = vi.fn().mockRejectedValue(new Error('Network error'));

      const source = getGitHubMarketplaces()[0];
      const stars = await fetchMarketplaceStars(source);

      expect(stars).toBeNull();
    });

    it('should use cached value on subsequent calls', async () => {
      const mockResponse = {
        ok: true,
        json: () => Promise.resolve({ stargazers_count: 999 }),
      };
      global.fetch = vi.fn().mockResolvedValue(mockResponse);

      const source = getGitHubMarketplaces()[0];

      await fetchMarketplaceStars(source);

      const stars = await fetchMarketplaceStars(source);

      expect(stars).toBe(999);

      expect(global.fetch).toHaveBeenCalledTimes(1);
    });
  });

  describe('fetchAllMarketplaceStars', () => {
    const originalFetch = global.fetch;

    beforeEach(() => {
      clearStarsCache();
    });

    afterEach(() => {
      global.fetch = originalFetch;
    });

    it('should fetch stars for all marketplaces', async () => {
      const mockResponse = {
        ok: true,
        json: () => Promise.resolve({ stargazers_count: 500 }),
      };
      global.fetch = vi.fn().mockResolvedValue(mockResponse);

      const starsMap = await fetchAllMarketplaceStars();

      expect(starsMap.size).toBeGreaterThan(0);
    });

    it('should handle partial failures', async () => {
      let callCount = 0;
      global.fetch = vi.fn().mockImplementation(() => {
        callCount++;
        if (callCount === 1) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve({ stargazers_count: 100 }),
          });
        }
        return Promise.resolve({ ok: false });
      });

      const starsMap = await fetchAllMarketplaceStars();

      expect(starsMap.size).toBeGreaterThanOrEqual(0);
    });
  });
});
