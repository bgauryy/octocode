import { z } from 'zod';
import { GitHubSearchQuerySchema as SharedGitHubSearchQuerySchema } from '../../toolContract/schemas.js';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';

function hasText(value: unknown): boolean {
  return typeof value === 'string' && value.trim().length > 0;
}

function hasTerms(value: unknown): boolean {
  return Array.isArray(value) && value.some(hasText);
}

function hasRunnableSearchConstraint(query: Record<string, unknown>): boolean {
  if (query.operation === 'code') {
    return (
      hasTerms(query.keywords) ||
      ['owner', 'path', 'extension', 'filename', 'language'].some(field =>
        hasText(query[field])
      )
    );
  }
  if (query.operation === 'repositories') {
    return (
      hasTerms(query.keywords) ||
      hasTerms(query.topics) ||
      [
        'owner',
        'language',
        'stars',
        'forks',
        'goodFirstIssues',
        'updated',
        'created',
        'size',
        'visibility',
        'license',
      ].some(field => hasText(query[field])) ||
      typeof query.archived === 'boolean'
    );
  }
  return true;
}

export const GitHubSearchQuerySchema =
  SharedGitHubSearchQuerySchema.superRefine((query, ctx) => {
    if (hasRunnableSearchConstraint(query)) return;
    ctx.addIssue({
      code: 'custom',
      path: ['operation'],
      message: `${query.operation} needs at least one search term or scope filter`,
    });
  });
export type GitHubSearchQuery = z.infer<typeof GitHubSearchQuerySchema>;

export const GitHubSearchBulkQuerySchema = createRelaxedBulkQuerySchema(
  GitHubSearchQuerySchema,
  { maxQueries: 5 }
);
