import { describe, it, expect } from 'vitest';
import {
  ALL_TOOLS,
  GITHUB_SEARCH_CODE,
  GITHUB_FETCH_CONTENT,
  GITHUB_VIEW_REPO_STRUCTURE,
  GITHUB_SEARCH_REPOSITORIES,
  GITHUB_SEARCH_PULL_REQUESTS,
  PACKAGE_SEARCH,
  GITHUB_CLONE_REPO,
  LOCAL_RIPGREP,
  LOCAL_VIEW_STRUCTURE,
  LOCAL_FIND_FILES,
  LOCAL_FETCH_CONTENT,
} from '../../src/tools/toolConfig.js';
import {
  TOOL_NAMES,
  DESCRIPTIONS,
} from '../../src/tools/toolMetadata/proxies.js';

describe('Tool Configuration', () => {
  describe('ALL_TOOLS', () => {
    it('should contain all expected tools (6 GitHub + 1 Clone + 4 Local + 3 LSP = 14)', () => {
      expect(ALL_TOOLS).toHaveLength(14);

      const toolNames = ALL_TOOLS.map(t => t.name);

      expect(toolNames).toContain(TOOL_NAMES.GITHUB_SEARCH_CODE);
      expect(toolNames).toContain(TOOL_NAMES.GITHUB_FETCH_CONTENT);
      expect(toolNames).toContain(TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE);
      expect(toolNames).toContain(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES);
      expect(toolNames).toContain(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS);
      expect(toolNames).toContain(TOOL_NAMES.PACKAGE_SEARCH);

      expect(toolNames).toContain(TOOL_NAMES.LOCAL_RIPGREP);
      expect(toolNames).toContain(TOOL_NAMES.LOCAL_VIEW_STRUCTURE);
      expect(toolNames).toContain(TOOL_NAMES.LOCAL_FIND_FILES);
      expect(toolNames).toContain(TOOL_NAMES.LOCAL_FETCH_CONTENT);
    });

    it('should have all tools marked as default', () => {
      ALL_TOOLS.forEach(tool => {
        expect(tool.isDefault).toBe(true);
      });
    });

    it('should have valid tool types', () => {
      const validTypes = ['search', 'content', 'history', 'debug'];
      ALL_TOOLS.forEach(tool => {
        expect(validTypes).toContain(tool.type);
      });
    });

    it('should have isLocal correctly set for GitHub tools', () => {
      const githubTools = ALL_TOOLS.filter(t => !t.isLocal);
      expect(githubTools).toHaveLength(6);
      githubTools.forEach(tool => {
        expect(tool.isLocal).toBe(false);
      });
    });

    it('should have isLocal correctly set for Local tools', () => {
      const localTools = ALL_TOOLS.filter(t => t.isLocal);
      expect(localTools).toHaveLength(8);
      localTools.forEach(tool => {
        expect(tool.isLocal).toBe(true);
      });
    });
  });

  describe('GitHub tool configs', () => {
    it('GITHUB_SEARCH_CODE should have correct config', () => {
      expect(GITHUB_SEARCH_CODE.name).toBe(TOOL_NAMES.GITHUB_SEARCH_CODE);
      expect(GITHUB_SEARCH_CODE.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.GITHUB_SEARCH_CODE]
      );
      expect(GITHUB_SEARCH_CODE.type).toBe('search');
      expect(GITHUB_SEARCH_CODE.isLocal).toBe(false);
      expect(GITHUB_SEARCH_CODE.fn).toBeTypeOf('function');
    });

    it('GITHUB_FETCH_CONTENT should have correct config', () => {
      expect(GITHUB_FETCH_CONTENT.name).toBe(TOOL_NAMES.GITHUB_FETCH_CONTENT);
      expect(GITHUB_FETCH_CONTENT.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.GITHUB_FETCH_CONTENT]
      );
      expect(GITHUB_FETCH_CONTENT.type).toBe('content');
      expect(GITHUB_FETCH_CONTENT.isLocal).toBe(false);
      expect(GITHUB_FETCH_CONTENT.fn).toBeTypeOf('function');
    });

    it('GITHUB_VIEW_REPO_STRUCTURE should have correct config', () => {
      expect(GITHUB_VIEW_REPO_STRUCTURE.name).toBe(
        TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE
      );
      expect(GITHUB_VIEW_REPO_STRUCTURE.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]
      );
      expect(GITHUB_VIEW_REPO_STRUCTURE.type).toBe('content');
      expect(GITHUB_VIEW_REPO_STRUCTURE.isLocal).toBe(false);
      expect(GITHUB_VIEW_REPO_STRUCTURE.fn).toBeTypeOf('function');
    });

    it('GITHUB_SEARCH_REPOSITORIES should have correct config', () => {
      expect(GITHUB_SEARCH_REPOSITORIES.name).toBe(
        TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES
      );
      expect(GITHUB_SEARCH_REPOSITORIES.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]
      );
      expect(GITHUB_SEARCH_REPOSITORIES.type).toBe('search');
      expect(GITHUB_SEARCH_REPOSITORIES.isLocal).toBe(false);
      expect(GITHUB_SEARCH_REPOSITORIES.fn).toBeTypeOf('function');
    });

    it('GITHUB_SEARCH_PULL_REQUESTS should have correct config', () => {
      expect(GITHUB_SEARCH_PULL_REQUESTS.name).toBe(
        TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
      );
      expect(GITHUB_SEARCH_PULL_REQUESTS.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]
      );
      expect(GITHUB_SEARCH_PULL_REQUESTS.type).toBe('history');
      expect(GITHUB_SEARCH_PULL_REQUESTS.isLocal).toBe(false);
      expect(GITHUB_SEARCH_PULL_REQUESTS.fn).toBeTypeOf('function');
    });

    it('PACKAGE_SEARCH should have correct config', () => {
      expect(PACKAGE_SEARCH.name).toBe(TOOL_NAMES.PACKAGE_SEARCH);
      expect(PACKAGE_SEARCH.description).toBe(
        DESCRIPTIONS[TOOL_NAMES.PACKAGE_SEARCH]
      );
      expect(PACKAGE_SEARCH.type).toBe('search');
      expect(PACKAGE_SEARCH.isLocal).toBe(false);
      expect(PACKAGE_SEARCH.fn).toBeTypeOf('function');
    });
  });

  describe('Local tool configs', () => {
    it('LOCAL_RIPGREP should have correct config', () => {
      expect(LOCAL_RIPGREP.name).toBe(TOOL_NAMES.LOCAL_RIPGREP);
      expect(LOCAL_RIPGREP.type).toBe('search');
      expect(LOCAL_RIPGREP.isLocal).toBe(true);
      expect(LOCAL_RIPGREP.fn).toBeTypeOf('function');
    });

    it('LOCAL_VIEW_STRUCTURE should have correct config', () => {
      expect(LOCAL_VIEW_STRUCTURE.name).toBe(TOOL_NAMES.LOCAL_VIEW_STRUCTURE);
      expect(LOCAL_VIEW_STRUCTURE.type).toBe('content');
      expect(LOCAL_VIEW_STRUCTURE.isLocal).toBe(true);
      expect(LOCAL_VIEW_STRUCTURE.fn).toBeTypeOf('function');
    });

    it('LOCAL_FIND_FILES should have correct config', () => {
      expect(LOCAL_FIND_FILES.name).toBe(TOOL_NAMES.LOCAL_FIND_FILES);
      expect(LOCAL_FIND_FILES.type).toBe('search');
      expect(LOCAL_FIND_FILES.isLocal).toBe(true);
      expect(LOCAL_FIND_FILES.fn).toBeTypeOf('function');
    });

    it('LOCAL_FETCH_CONTENT should have correct config', () => {
      expect(LOCAL_FETCH_CONTENT.name).toBe(TOOL_NAMES.LOCAL_FETCH_CONTENT);
      expect(LOCAL_FETCH_CONTENT.type).toBe('content');
      expect(LOCAL_FETCH_CONTENT.isLocal).toBe(true);
      expect(LOCAL_FETCH_CONTENT.fn).toBeTypeOf('function');
    });
  });

  describe('Clone tool config', () => {
    it('GITHUB_CLONE_REPO should have isClone: true', () => {
      expect(GITHUB_CLONE_REPO.isClone).toBe(true);
    });

    it('GITHUB_CLONE_REPO should have isLocal: true', () => {
      expect(GITHUB_CLONE_REPO.isLocal).toBe(true);
    });

    it('GITHUB_CLONE_REPO should have skipMetadataCheck: true', () => {
      expect(GITHUB_CLONE_REPO.skipMetadataCheck).toBe(true);
    });

    it('only GITHUB_CLONE_REPO should have isClone: true', () => {
      const cloneTools = ALL_TOOLS.filter(t => t.isClone);
      expect(cloneTools).toHaveLength(1);
      expect(cloneTools[0]!.name).toBe(TOOL_NAMES.GITHUB_CLONE_REPO);
    });

    it('non-clone tools should not have isClone set', () => {
      const nonCloneTools = ALL_TOOLS.filter(t => !t.isClone);
      expect(nonCloneTools).toHaveLength(13);
      nonCloneTools.forEach(tool => {
        expect(tool.isClone).toBeFalsy();
      });
    });
  });

  describe('getDescription fallback', () => {
    it('should return empty string for unknown tool names', () => {
      const unknownKey = 'unknown_tool_that_does_not_exist';
      expect(DESCRIPTIONS[unknownKey]).toBe('');
    });
  });
});
