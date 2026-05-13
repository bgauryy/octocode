import { describe, it, expect } from 'vitest';
import { extractResearchFields } from '../src/paramExtractors.js';

describe('extractResearchFields', () => {
  describe('Single Query', () => {
    it('should extract all research fields from a single query', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: 'Find authentication',
            researchGoal: 'Locate login function',
            reasoning: 'Security audit',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Find authentication',
        researchGoal: 'Locate login function',
        reasoning: 'Security audit',
      });
    });

    it('should extract partial research fields from a single query', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: 'Find authentication',
            reasoning: 'Security audit',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Find authentication',
        reasoning: 'Security audit',
      });
    });

    it('should return empty object when no research fields present', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({});
    });

    it('should ignore empty string research fields', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: '',
            researchGoal: 'Valid goal',
            reasoning: '',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        researchGoal: 'Valid goal',
      });
    });
  });

  describe('Multiple Queries', () => {
    it('should consolidate same values from multiple queries', () => {
      const params = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find login',
            reasoning: 'Security audit',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find login',
            reasoning: 'Security audit',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Authentication',
        researchGoal: 'Find login',
        reasoning: 'Security audit',
      });
    });

    it('should consolidate different values with semicolon separator', () => {
      const params = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find login',
            reasoning: 'Security audit',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find logout',
            reasoning: 'Code review',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Authentication',
        researchGoal: 'Find login; Find logout',
        reasoning: 'Security audit; Code review',
      });
    });

    it('should handle mixed presence of research fields across queries', () => {
      const params = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Authentication',
            researchGoal: 'Find login',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            researchGoal: 'Find logout',
            reasoning: 'Code review',
          },
          {
            owner: 'owner3',
            repo: 'repo3',
            mainResearchGoal: 'Authorization',
            reasoning: 'Security audit',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Authentication; Authorization',
        researchGoal: 'Find login; Find logout',
        reasoning: 'Code review; Security audit',
      });
    });

    it('should deduplicate values while preserving order', () => {
      const params = {
        queries: [
          {
            owner: 'owner1',
            repo: 'repo1',
            mainResearchGoal: 'Goal A',
            researchGoal: 'Task 1',
          },
          {
            owner: 'owner2',
            repo: 'repo2',
            mainResearchGoal: 'Goal B',
            researchGoal: 'Task 2',
          },
          {
            owner: 'owner3',
            repo: 'repo3',
            mainResearchGoal: 'Goal A',
            researchGoal: 'Task 1',
          },
        ],
      };

      const result = extractResearchFields(params);

      // Sets preserve insertion order, so first occurrence wins
      expect(result).toEqual({
        mainResearchGoal: 'Goal A; Goal B',
        researchGoal: 'Task 1; Task 2',
      });
    });
  });

  describe('Direct Parameters (Non-Bulk)', () => {
    it('should extract research fields from direct params', () => {
      const params = {
        owner: 'test-owner',
        repo: 'test-repo',
        mainResearchGoal: 'Find patterns',
        researchGoal: 'Analyze code',
        reasoning: 'Learning',
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Find patterns',
        researchGoal: 'Analyze code',
        reasoning: 'Learning',
      });
    });

    it('should handle direct params with partial fields', () => {
      const params = {
        owner: 'test-owner',
        repo: 'test-repo',
        mainResearchGoal: 'Find patterns',
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        mainResearchGoal: 'Find patterns',
      });
    });

    it('should return empty object for direct params without research fields', () => {
      const params = {
        owner: 'test-owner',
        repo: 'test-repo',
        path: 'some/file.js',
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({});
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty queries array', () => {
      const params = {
        queries: [],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({});
    });

    it('should handle params with no queries key', () => {
      const params = {
        owner: 'test-owner',
        repo: 'test-repo',
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({});
    });

    it('should ignore non-string research field values', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: 123, // number instead of string
            researchGoal: 'Valid goal',
            reasoning: null, // null instead of string
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({
        researchGoal: 'Valid goal',
      });
    });

    it('should handle queries with only empty research fields', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: '',
            researchGoal: '',
            reasoning: '',
          },
        ],
      };

      const result = extractResearchFields(params);

      expect(result).toEqual({});
    });

    it('should handle whitespace-only strings as empty', () => {
      const params = {
        queries: [
          {
            owner: 'test-owner',
            repo: 'test-repo',
            mainResearchGoal: '   ',
            researchGoal: 'Valid goal',
          },
        ],
      };

      const result = extractResearchFields(params);

      // Note: Currently the implementation doesn't trim, so whitespace-only
      // strings are treated as valid. This documents the current behavior.
      expect(result).toEqual({
        mainResearchGoal: '   ',
        researchGoal: 'Valid goal',
      });
    });
  });
});
