import { GitHubPullRequestsSearchParams } from '../githubAPI.js';
import { BaseQueryBuilder } from './base.js';

export class PullRequestSearchQueryBuilder extends BaseQueryBuilder {
  addBasicFilters(params: GitHubPullRequestsSearchParams): this {
    if (params.query && params.query.trim()) {
      this.queryParts.push(params.query.trim());

      if (params.match && params.match.length > 0) {
        this.queryParts.push(`in:${params.match.join(',')}`);
      }
    }

    this.queryParts.push('is:pr');
    return this;
  }

  addStateFilters(params: GitHubPullRequestsSearchParams): this {
    // `is:merged` already implies a closed PR. The caller maps state:"merged"
    // to state:"closed" + merged:true (needed for the REST path), but in the
    // search query that yields a redundant, over-constrained `is:closed
    // is:merged` — emit only `is:merged`.
    const redundantClosed = params.merged === true && params.state === 'closed';
    if (!redundantClosed) {
      this.addSimpleFilter(params.state, 'is');
    }
    this.addBooleanFilter(params.draft, 'is:draft', '-is:draft');
    this.addBooleanFilter(params.merged, 'is:merged', 'is:unmerged');
    return this;
  }

  addUserFilters(params: GitHubPullRequestsSearchParams): this {
    this.addSimpleFilter(params.author, 'author');
    this.addSimpleFilter(params.assignee, 'assignee');
    this.addSimpleFilter(params.mentions, 'mentions');
    this.addSimpleFilter(params.commenter, 'commenter');
    this.addSimpleFilter(params.involves, 'involves');
    this.addSimpleFilter(params['reviewed-by'], 'reviewed-by');
    this.addSimpleFilter(params['review-requested'], 'review-requested');
    return this;
  }

  addBranchFilters(params: GitHubPullRequestsSearchParams): this {
    this.addSimpleFilter(params.head, 'head');
    this.addSimpleFilter(params.base, 'base');
    return this;
  }

  addReviewFilters(params: GitHubPullRequestsSearchParams): this {
    if (params.review) this.queryParts.push(`review:${params.review}`);
    return this;
  }

  addNegativeFilters(params: GitHubPullRequestsSearchParams): this {
    if (params['no-assignee']) this.queryParts.push('no:assignee');
    if (params['no-label']) this.queryParts.push('no:label');
    if (params['no-milestone']) this.queryParts.push('no:milestone');
    if (params['no-project']) this.queryParts.push('no:project');
    if (params.locked === true) this.queryParts.push('is:locked');
    else if (params.locked === false) this.queryParts.push('is:unlocked');
    if (params.visibility === 'public') this.queryParts.push('is:public');
    else if (params.visibility === 'private')
      this.queryParts.push('is:private');
    if (params['team-mentions'])
      this.queryParts.push(`team:${params['team-mentions']}`);
    if (params.project) this.queryParts.push(`project:${params.project}`);
    return this;
  }

  addMiscFilters(params: GitHubPullRequestsSearchParams): this {
    this.queryParts.push(
      params.archived === true ? 'archived:true' : 'archived:false'
    );
    if (params.language) this.queryParts.push(`language:${params.language}`);
    if (params.checks) this.queryParts.push(`status:${params.checks}`);
    return this;
  }
}

export function buildPullRequestSearchQuery(
  params: GitHubPullRequestsSearchParams
): string {
  return new PullRequestSearchQueryBuilder()
    .addBasicFilters(params)
    .addOwnerRepo(params)
    .addStateFilters(params)
    .addUserFilters(params)
    .addBranchFilters(params)
    .addDateFilters(params)
    .addEngagementFilters(params)
    .addReviewFilters(params)
    .addOrganizationFilters(params)
    .addNegativeFilters(params)
    .addMiscFilters(params)
    .build();
}

export function shouldUseSearchForPRs(
  params: GitHubPullRequestsSearchParams
): boolean {
  return (
    params.draft !== undefined ||
    params.author !== undefined ||
    params.assignee !== undefined ||
    (typeof params.query === 'string' && params.query.trim().length > 0) ||
    (params.label && params.label.length > 0) ||
    params.mentions !== undefined ||
    params.commenter !== undefined ||
    params.involves !== undefined ||
    params['reviewed-by'] !== undefined ||
    params['review-requested'] !== undefined ||
    params.reactions !== undefined ||
    params.comments !== undefined ||
    params.interactions !== undefined ||
    params['no-assignee'] !== undefined ||
    params['no-label'] !== undefined ||
    params['no-milestone'] !== undefined ||
    params['no-project'] !== undefined ||
    params.state === 'merged' ||
    params.milestone !== undefined ||
    params.language !== undefined ||
    params.checks !== undefined ||
    params.review !== undefined ||
    params.locked !== undefined ||
    params.visibility !== undefined ||
    params['team-mentions'] !== undefined ||
    params.project !== undefined ||
    params.created !== undefined ||
    params.updated !== undefined ||
    params['merged-at'] !== undefined ||
    params.closed !== undefined ||
    params.merged !== undefined ||
    (params.match !== undefined && params.match.length > 0) ||
    params.sort === 'comments' ||
    params.sort === 'reactions' ||
    Array.isArray(params.owner) ||
    Array.isArray(params.repo)
  );
}
