import type { GitHubPullRequestsSearchParams } from '../githubAPI.js';

export function getOwnerQualifier(owner: string): string {
  return `user:${owner}`;
}

const GITHUB_SEARCH_SPECIAL_CHARS = /[@/]/;

const GITHUB_BARE_KEYWORD = /^[A-Za-z0-9_-]+$/;

export function quoteKeywordIfNeeded(keyword: string): string {
  if (keyword.startsWith('"')) {
    return keyword;
  }
  if (!GITHUB_BARE_KEYWORD.test(keyword)) {
    return `"${keyword.replace(/"/g, '\\"')}"`;
  }
  return keyword;
}

export abstract class BaseQueryBuilder {
  protected queryParts: string[] = [];

  addOwnerRepo(params: {
    owner?: string | string[] | null;
    repo?: string | string[] | null;
  }): this {
    if (params.owner && params.repo) {
      const owners = Array.isArray(params.owner)
        ? params.owner
        : [params.owner];
      const repos = Array.isArray(params.repo) ? params.repo : [params.repo];

      owners.forEach(owner => {
        repos.forEach(repo => {
          this.queryParts.push(`repo:${owner}/${repo}`);
        });
      });
    } else if (params.owner) {
      const owners = Array.isArray(params.owner)
        ? params.owner
        : [params.owner];
      owners.forEach(owner => {
        this.queryParts.push(getOwnerQualifier(owner));
      });
    }
    return this;
  }

  addDateFilters(
    params: Record<string, unknown> | GitHubPullRequestsSearchParams
  ): this {
    const dateFields: Record<string, string> = {
      created: 'created',
      updated: 'updated',
      'author-date': 'author-date',
      'committer-date': 'committer-date',
      'merged-at': 'merged',
      closed: 'closed',
    };

    Object.entries(dateFields).forEach(([paramKey, queryKey]) => {
      const value = (params as Record<string, unknown>)[paramKey];
      if (value) {
        this.queryParts.push(`${queryKey}:${value}`);
      }
    });
    return this;
  }

  addArrayFilter(
    values: string | string[] | null | undefined,
    prefix: string,
    quoted = false
  ): this {
    if (values && values !== null) {
      const valueArray = Array.isArray(values) ? values : [values];
      valueArray.forEach(value => {
        const formattedValue = quoted ? `"${value}"` : value;
        this.queryParts.push(`${prefix}:${formattedValue}`);
      });
    }
    return this;
  }

  addBooleanFilter(
    value: boolean | undefined,
    trueQuery: string,
    falseQuery: string
  ): this {
    if (value === true) {
      this.queryParts.push(trueQuery);
    } else if (value === false) {
      this.queryParts.push(falseQuery);
    }
    return this;
  }

  addSimpleFilter(
    value: string | number | null | undefined,
    key: string
  ): this {
    if (value !== undefined && value !== null) {
      this.queryParts.push(`${key}:${value}`);
    }
    return this;
  }

  addQuotedFilter(value: string | null | undefined, key: string): this {
    if (value !== undefined && value !== null) {
      const needsQuoting =
        GITHUB_SEARCH_SPECIAL_CHARS.test(value) && !value.startsWith('"');
      const formatted = needsQuoting ? `"${value}"` : value;
      this.queryParts.push(`${key}:${formatted}`);
    }
    return this;
  }

  addEngagementFilters(params: {
    comments?: number | string;
    reactions?: number | string;
    interactions?: number | string;
  }): this {
    this.addSimpleFilter(params.comments, 'comments');
    this.addSimpleFilter(params.reactions, 'reactions');
    this.addSimpleFilter(params.interactions, 'interactions');
    return this;
  }

  addOrganizationFilters(params: {
    label?: string | string[];
    milestone?: string;
  }): this {
    this.addArrayFilter(params.label, 'label', true);
    if (params.milestone) {
      this.queryParts.push(`milestone:"${params.milestone}"`);
    }
    return this;
  }

  build(): string {
    return this.queryParts.join(' ').trim();
  }
}
