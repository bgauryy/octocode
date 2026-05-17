/**
 * Branch coverage tests for src/scheme/remoteSchemaOverlay.ts
 *
 * Covers:
 * - packageQueryWithEcosystemDefault preprocess:
 *   - val has no `ecosystem` → injects 'npm'
 *   - val already has `ecosystem` → passes through unchanged
 *   - val is null → passes through
 *   - val is non-object → passes through
 * - GitHubPullRequestSearchQueryLocalSchema with 'merged' state extension
 */

import { describe, it, expect } from 'vitest';
import {
  PackageSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';

const BASE_RESEARCH = {
  id: 'test:remote-overlay',
  mainResearchGoal: 'Test remoteSchemaOverlay',
  researchGoal: 'Test preprocess branches',
  reasoning: 'Branch coverage',
};

const QUERY_BASE = {
  id: 'q1',
  mainResearchGoal: 'Test remoteSchemaOverlay',
  researchGoal: 'Test preprocess branches',
  reasoning: 'Branch coverage',
};

describe('remoteSchemaOverlay', () => {
  describe('PackageSearchBulkQueryLocalSchema — ecosystem preprocess', () => {
    it('injects ecosystem=npm when the field is absent', () => {
      const result = PackageSearchBulkQueryLocalSchema.safeParse({
        queries: [{ ...QUERY_BASE, name: 'lodash' }],
      });

      expect(result.success).toBe(true);
    });

    it('passes through unchanged when ecosystem is already set to npm', () => {
      const result = PackageSearchBulkQueryLocalSchema.safeParse({
        queries: [{ ...QUERY_BASE, name: 'lodash', ecosystem: 'npm' }],
      });

      expect(result.success).toBe(true);
    });

    it('passes through (does not inject npm) when ecosystem field is present but invalid', () => {
      // preprocess sees `ecosystem` exists → skips injection → schema rejects invalid ecosystem
      const result = PackageSearchBulkQueryLocalSchema.safeParse({
        queries: [{ ...QUERY_BASE, name: 'lodash', ecosystem: 'invalid' }],
      });

      expect(result.success).toBe(false);
    });

    it('rejects invalid input gracefully (null query entry)', () => {
      const result = PackageSearchBulkQueryLocalSchema.safeParse({
        queries: [null],
      });

      expect(result.success).toBe(false);
    });
  });

  describe('GitHubPullRequestSearchBulkQueryLocalSchema — merged state', () => {
    it('accepts state: merged (local extension)', () => {
      const result = GitHubPullRequestSearchBulkQueryLocalSchema.safeParse({
        queries: [
          { ...QUERY_BASE, owner: 'facebook', repo: 'react', state: 'merged' },
        ],
      });

      expect(result.success).toBe(true);
    });

    it('accepts state: open (unchanged from upstream)', () => {
      const result = GitHubPullRequestSearchBulkQueryLocalSchema.safeParse({
        queries: [
          { ...QUERY_BASE, owner: 'facebook', repo: 'react', state: 'open' },
        ],
      });

      expect(result.success).toBe(true);
    });

    it('accepts state: closed (unchanged from upstream)', () => {
      const result = GitHubPullRequestSearchBulkQueryLocalSchema.safeParse({
        queries: [
          { ...QUERY_BASE, owner: 'facebook', repo: 'react', state: 'closed' },
        ],
      });

      expect(result.success).toBe(true);
    });

    it('rejects invalid state value', () => {
      const result = GitHubPullRequestSearchBulkQueryLocalSchema.safeParse({
        queries: [
          {
            ...QUERY_BASE,
            owner: 'facebook',
            repo: 'react',
            state: 'invalid-state',
          },
        ],
      });

      expect(result.success).toBe(false);
    });
  });
});
