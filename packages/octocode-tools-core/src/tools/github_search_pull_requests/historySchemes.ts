import { z } from 'zod';
import { validateCommitKeywordScope } from '../../toolContract/input/resources/tools/historyCommitInput.js';

import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import { getRequiredSchemaField } from '../../scheme/conditionalSchemas.js';
import {
  CommitCompareQueryShape,
  CommitHistoryQueryShape,
  IssueDetailQueryShape,
  IssueListQueryShape,
  PullRequestDetailQueryShape,
  PullRequestListQueryShape,
} from './splitSchemes.js';

const operation = <T extends string>(value: T) =>
  z.literal(value).describe('Required operation selector.');

const requiredOwner = (shape: z.ZodRawShape) =>
  getRequiredSchemaField(shape, 'owner');
const requiredRepo = (shape: z.ZodRawShape) =>
  getRequiredSchemaField(shape, 'repo');

const pullRequestsSearchSchema = PullRequestListQueryShape.extend({
  operation: operation('pullRequests'),
}).strict();

const issuesSearchSchema = IssueListQueryShape.extend({
  operation: operation('issues'),
  owner: requiredOwner(IssueListQueryShape.shape),
  repo: requiredRepo(IssueListQueryShape.shape),
}).strict();

const commitsSearchShape = CommitHistoryQueryShape.omit({
  includeDiff: true,
  filePage: true,
  charOffset: true,
  charLength: true,
});
const commitsSearchSchema = commitsSearchShape
  .extend({
    operation: operation('commits'),
    owner: requiredOwner(commitsSearchShape.shape),
    repo: requiredRepo(commitsSearchShape.shape),
  })
  .strict()
  .superRefine(validateCommitKeywordScope);

export const GitHubSearchHistoryQueryLocalSchema = z
  .discriminatedUnion('operation', [
    pullRequestsSearchSchema,
    issuesSearchSchema,
    commitsSearchSchema,
  ])
  .describe(
    'Search or list GitHub pull requests, issues, or commits using one strict operation branch.'
  );

export const GitHubSearchHistoryBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubSearchHistoryQueryLocalSchema);

const pullRequestItemShape = PullRequestDetailQueryShape.omit({
  prNumber: true,
});
const pullRequestItemSchema = pullRequestItemShape
  .extend({
    operation: operation('pullRequest'),
    owner: requiredOwner(pullRequestItemShape.shape),
    repo: requiredRepo(pullRequestItemShape.shape),
    number: PullRequestDetailQueryShape.shape.prNumber,
  })
  .strict();

const issueItemShape = IssueDetailQueryShape.omit({
  issueNumber: true,
  matchString: true,
  commentBodyOffset: true,
  minify: true,
});
const issueItemSchema = issueItemShape
  .extend({
    operation: operation('issue'),
    owner: requiredOwner(issueItemShape.shape),
    repo: requiredRepo(issueItemShape.shape),
    number: IssueDetailQueryShape.shape.issueNumber,
  })
  .strict();

const commitItemShape = CommitCompareQueryShape.omit({
  base: true,
  head: true,
  page: true,
});
const commitItemSchema = commitItemShape
  .extend({
    operation: operation('commit'),
    owner: requiredOwner(commitItemShape.shape),
    repo: requiredRepo(commitItemShape.shape),
    ref: z.string().min(1).describe('Commit SHA or ref to retrieve exactly.'),
    fileBatch: z
      .number()
      .int()
      .min(1)
      .max(30)
      .optional()
      .describe('Provider file batch from next.nextFilePage; copy unchanged.'),
  })
  .strict();

const compareItemSchema = CommitCompareQueryShape.extend({
  operation: operation('compare'),
  owner: requiredOwner(CommitCompareQueryShape.shape),
  repo: requiredRepo(CommitCompareQueryShape.shape),
}).strict();

export const GitHubGetHistoryItemQueryLocalSchema = z
  .discriminatedUnion('operation', [
    pullRequestItemSchema,
    issueItemSchema,
    commitItemSchema,
    compareItemSchema,
  ])
  .describe(
    'Retrieve one GitHub pull request, issue, commit, or ref comparison using one strict operation branch.'
  );

export const GitHubGetHistoryItemBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubGetHistoryItemQueryLocalSchema);
