import { describe, it, expect } from 'vitest';

import { TOOL_NAMES } from '../../src/tools/toolMetadata/proxies.js';
import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import { isLocalTool } from '../../src/tools/toolNames.js';

describe('TOOL_NAMES proxy (TDD for local tools registration)', () => {
  describe('before metadata initialization', () => {
    it('should return correct value for LOCAL_RIPGREP from STATIC_TOOL_NAMES', () => {
      expect(TOOL_NAMES.LOCAL_RIPGREP).toBe('localSearchCode');
      expect(TOOL_NAMES.LOCAL_RIPGREP).toBe(STATIC_TOOL_NAMES.LOCAL_RIPGREP);
    });

    it('should return correct value for LOCAL_VIEW_STRUCTURE from STATIC_TOOL_NAMES', () => {
      expect(TOOL_NAMES.LOCAL_VIEW_STRUCTURE).toBe('localViewStructure');
      expect(TOOL_NAMES.LOCAL_VIEW_STRUCTURE).toBe(
        STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE
      );
    });

    it('should return correct value for LOCAL_FIND_FILES from STATIC_TOOL_NAMES', () => {
      expect(TOOL_NAMES.LOCAL_FIND_FILES).toBe('localFindFiles');
      expect(TOOL_NAMES.LOCAL_FIND_FILES).toBe(
        STATIC_TOOL_NAMES.LOCAL_FIND_FILES
      );
    });

    it('should return correct value for LOCAL_FETCH_CONTENT from STATIC_TOOL_NAMES', () => {
      expect(TOOL_NAMES.LOCAL_FETCH_CONTENT).toBe('localGetFileContent');
      expect(TOOL_NAMES.LOCAL_FETCH_CONTENT).toBe(
        STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT
      );
    });

    it('should return correct value for all GitHub tools', () => {
      expect(TOOL_NAMES.GITHUB_FETCH_CONTENT).toBe('githubGetFileContent');
      expect(TOOL_NAMES.GITHUB_SEARCH_CODE).toBe('githubSearchCode');
      expect(TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES).toBe(
        'githubSearchRepositories'
      );
      expect(TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE).toBe(
        'githubViewRepoStructure'
      );
      expect(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS).toBe(
        'githubSearchPullRequests'
      );
      expect(TOOL_NAMES.PACKAGE_SEARCH).toBe('packageSearch');
    });

    it('should not return undefined for any defined tool', () => {
      expect(TOOL_NAMES.LOCAL_RIPGREP).not.toBeUndefined();
      expect(TOOL_NAMES.LOCAL_VIEW_STRUCTURE).not.toBeUndefined();
      expect(TOOL_NAMES.LOCAL_FIND_FILES).not.toBeUndefined();
      expect(TOOL_NAMES.LOCAL_FETCH_CONTENT).not.toBeUndefined();
    });

    it('TOOL_NAMES should be enumerable with correct keys', () => {
      const keys = Object.keys(TOOL_NAMES);
      expect(keys).toContain('LOCAL_RIPGREP');
      expect(keys).toContain('LOCAL_VIEW_STRUCTURE');
      expect(keys).toContain('LOCAL_FIND_FILES');
      expect(keys).toContain('LOCAL_FETCH_CONTENT');
    });
  });

  describe('STATIC_TOOL_NAMES consistency', () => {
    it('should have all local tool names defined', () => {
      expect(STATIC_TOOL_NAMES.LOCAL_RIPGREP).toBe('localSearchCode');
      expect(STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE).toBe('localViewStructure');
      expect(STATIC_TOOL_NAMES.LOCAL_FIND_FILES).toBe('localFindFiles');
      expect(STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT).toBe('localGetFileContent');
    });
  });
});

describe('isLocalTool', () => {
  describe('local tools', () => {
    it('should return true for localSearchCode', () => {
      expect(isLocalTool('localSearchCode')).toBe(true);
    });

    it('should return true for localGetFileContent', () => {
      expect(isLocalTool('localGetFileContent')).toBe(true);
    });

    it('should return true for localFindFiles', () => {
      expect(isLocalTool('localFindFiles')).toBe(true);
    });

    it('should return true for localViewStructure', () => {
      expect(isLocalTool('localViewStructure')).toBe(true);
    });
  });

  describe('GitHub tools', () => {
    it('should return false for githubSearchCode', () => {
      expect(isLocalTool('githubSearchCode')).toBe(false);
    });

    it('should return false for githubGetFileContent', () => {
      expect(isLocalTool('githubGetFileContent')).toBe(false);
    });

    it('should return false for githubViewRepoStructure', () => {
      expect(isLocalTool('githubViewRepoStructure')).toBe(false);
    });

    it('should return false for githubSearchRepositories', () => {
      expect(isLocalTool('githubSearchRepositories')).toBe(false);
    });

    it('should return false for githubSearchPullRequests', () => {
      expect(isLocalTool('githubSearchPullRequests')).toBe(false);
    });
  });

  describe('other tools', () => {
    it('should return false for packageSearch', () => {
      expect(isLocalTool('packageSearch')).toBe(false);
    });

    it('should return false for unknown tool names', () => {
      expect(isLocalTool('unknownTool')).toBe(false);
    });
  });
});
