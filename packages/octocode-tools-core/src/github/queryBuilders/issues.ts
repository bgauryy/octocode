import { BaseQueryBuilder } from './base.js';

/** Issue-search params — PR-only fields (draft/review/head/base) are omitted. */
export type IssueSearchParams = {
  query?: string;
  owner?: string | string[];
  repo?: string | string[];
  state?: 'open' | 'closed' | 'merged';
  author?: string;
  assignee?: string;
  mentions?: string;
  commenter?: string;
  involves?: string;
  created?: string;
  updated?: string;
  closed?: string;
  comments?: number | string;
  reactions?: number | string;
  interactions?: number | string;
  label?: string | string[];
  milestone?: string;
  locked?: boolean;
  visibility?: 'public' | 'private';
  'no-assignee'?: boolean;
  'no-label'?: boolean;
  'no-milestone'?: boolean;
  'no-project'?: boolean;
  match?: ('title' | 'body' | 'comments')[];
  archived?: boolean;
  sort?: 'created' | 'updated' | 'best-match' | 'comments' | 'reactions';
  order?: 'asc' | 'desc';
  limit?: number;
  page?: number;
};

export class IssueSearchQueryBuilder extends BaseQueryBuilder {
  addBasicFilters(params: IssueSearchParams): this {
    if (params.query && params.query.trim()) {
      this.queryParts.push(params.query.trim());
      if (params.match && params.match.length > 0) {
        this.queryParts.push(`in:${params.match.join(',')}`);
      }
    }
    this.queryParts.push('is:issue');
    return this;
  }

  addStateFilters(params: IssueSearchParams): this {
    if (params.state === 'open' || params.state === 'closed') {
      this.addSimpleFilter(params.state, 'is');
    }
    return this;
  }

  addUserFilters(params: IssueSearchParams): this {
    this.addSimpleFilter(params.author, 'author');
    this.addSimpleFilter(params.assignee, 'assignee');
    this.addSimpleFilter(params.mentions, 'mentions');
    this.addSimpleFilter(params.commenter, 'commenter');
    this.addSimpleFilter(params.involves, 'involves');
    return this;
  }

  addNegativeFilters(params: IssueSearchParams): this {
    if (params['no-assignee']) this.queryParts.push('no:assignee');
    if (params['no-label']) this.queryParts.push('no:label');
    if (params['no-milestone']) this.queryParts.push('no:milestone');
    if (params['no-project']) this.queryParts.push('no:project');
    if (params.locked === true) this.queryParts.push('is:locked');
    else if (params.locked === false) this.queryParts.push('is:unlocked');
    if (params.visibility === 'public') this.queryParts.push('is:public');
    else if (params.visibility === 'private')
      this.queryParts.push('is:private');
    return this;
  }

  addMiscFilters(params: IssueSearchParams): this {
    this.queryParts.push(
      params.archived === true ? 'archived:true' : 'archived:false'
    );
    return this;
  }
}

export function buildIssueSearchQuery(params: IssueSearchParams): string {
  return new IssueSearchQueryBuilder()
    .addBasicFilters(params)
    .addOwnerRepo(params)
    .addStateFilters(params)
    .addUserFilters(params)
    .addDateFilters(params)
    .addEngagementFilters(params)
    .addOrganizationFilters(params)
    .addNegativeFilters(params)
    .addMiscFilters(params)
    .build();
}

export function shouldUseSearchForIssues(params: IssueSearchParams): boolean {
  return (
    (typeof params.query === 'string' && params.query.trim().length > 0) ||
    params.author !== undefined ||
    params.assignee !== undefined ||
    (params.label !== undefined &&
      (Array.isArray(params.label)
        ? params.label.length > 0
        : params.label.length > 0)) ||
    params.mentions !== undefined ||
    params.commenter !== undefined ||
    params.involves !== undefined ||
    params.reactions !== undefined ||
    params.comments !== undefined ||
    params.interactions !== undefined ||
    params['no-assignee'] !== undefined ||
    params['no-label'] !== undefined ||
    params['no-milestone'] !== undefined ||
    params['no-project'] !== undefined ||
    params.milestone !== undefined ||
    params.locked !== undefined ||
    params.visibility !== undefined ||
    params.created !== undefined ||
    params.updated !== undefined ||
    params.closed !== undefined ||
    (params.match !== undefined && params.match.length > 0) ||
    params.sort === 'comments' ||
    params.sort === 'reactions' ||
    Array.isArray(params.owner) ||
    Array.isArray(params.repo)
  );
}
