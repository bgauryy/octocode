import { describe, it, expect } from 'vitest';
import * as githubAPI from '../../../octocode-tools-core/src/github/githubAPI.js';

describe('GitHub API Types', () => {
  describe('Type guards', () => {
    it('should export isGitHubAPIError type guard', () => {
      expect(typeof githubAPI.isGitHubAPIError).toEqual('function');
    });

    it('should export isGitHubAPISuccess type guard', () => {
      expect(typeof githubAPI.isGitHubAPISuccess).toEqual('function');
    });

    it('should export isRepository type guard', () => {
      expect(typeof githubAPI.isRepository).toEqual('function');
    });
  });

  describe('Type guard functionality', () => {
    it('isGitHubAPIError should correctly identify error objects', () => {
      const error: githubAPI.GitHubAPIError = {
        error: 'Test error',
        type: 'http',
        status: 404,
      };

      expect(githubAPI.isGitHubAPIError(error)).toBe(true);
      expect(githubAPI.isGitHubAPIError({})).toBe(false);
      expect(githubAPI.isGitHubAPIError(null)).toBe(false);
      expect(githubAPI.isGitHubAPIError({ error: 'test' })).toBe(false);
    });

    it('isGitHubAPISuccess should correctly identify success objects', () => {
      const success: githubAPI.GitHubAPISuccess<{ test: string }> = {
        data: { test: 'value' },
        status: 200,
      };

      expect(githubAPI.isGitHubAPISuccess(success)).toBe(true);
      expect(githubAPI.isGitHubAPISuccess({})).toBe(false);
      expect(githubAPI.isGitHubAPISuccess(null)).toBe(false);
      expect(githubAPI.isGitHubAPISuccess({ data: 'test' })).toBe(false);
    });

    it('isRepository should correctly identify repository objects', () => {
      const repo = {
        id: 123,
        name: 'test-repo',
        full_name: 'owner/test-repo',
        private: false,
      };

      expect(githubAPI.isRepository(repo)).toBe(true);
      expect(githubAPI.isRepository({})).toBe(false);
      expect(githubAPI.isRepository(null)).toBe(false);
      expect(githubAPI.isRepository({ id: 123 })).toBe(false);
    });
  });
});
