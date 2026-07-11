import type { z } from 'zod';
import type {
  GitHubCodeSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
} from '@octocodeai/octocode-core/schemas';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import type { WithOptionalMeta } from '../../types/execution.js';
import { BaseQueryBuilder, quoteKeywordIfNeeded } from './base.js';

const FILE_PATH_TAIL = /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/;

export class CodeSearchQueryBuilder extends BaseQueryBuilder {
  addQueryTerms(params: WithOptionalMeta<GitHubCodeSearchQuery>): this {
    if (Array.isArray(params.keywords) && params.keywords.length > 0) {
      const nonEmptyTerms = params.keywords.filter(term => term && term.trim());
      if (nonEmptyTerms.length > 0) {
        this.queryParts.push(...nonEmptyTerms.map(quoteKeywordIfNeeded));
      }
    }
    return this;
  }

  addSearchFilters(params: WithOptionalMeta<GitHubCodeSearchQuery>): this {
    let path = params.path;
    let filename = params.filename;
    const fileTail =
      typeof path === 'string' && !filename ? path.match(FILE_PATH_TAIL) : null;
    if (fileTail) {
      filename = fileTail[1];
      path = path!.slice(0, fileTail.index) || undefined;
    }

    this.addSimpleFilter(filename, 'filename');
    this.addSimpleFilter(params.extension, 'extension');
    this.addQuotedFilter(path, 'path');
    if ((params as Record<string, unknown>).language) {
      this.queryParts.push(
        `language:${(params as Record<string, unknown>).language}`
      );
    }
    return this;
  }

  addMatchFilters(params: WithOptionalMeta<GitHubCodeSearchQuery>): this {
    if (params.match) {
      const matches = Array.isArray(params.match)
        ? params.match
        : [params.match];
      matches.forEach(match => {
        if (match === 'file') {
          this.queryParts.push('in:file');
        } else if (match === 'path') {
          this.queryParts.push('in:path');
        }
      });
    }
    return this;
  }
}

export class RepoSearchQueryBuilder extends BaseQueryBuilder {
  addQueryTerms(params: WithOptionalMeta<GitHubReposSearchSingleQuery>): this {
    if (Array.isArray(params.keywords) && params.keywords.length > 0) {
      this.queryParts.push(...params.keywords.map(quoteKeywordIfNeeded));
    }
    return this;
  }

  addRepoFilters(params: WithOptionalMeta<GitHubReposSearchSingleQuery>): this {
    this.addArrayFilter(params.topicsToSearch, 'topic');
    this.addSimpleFilter(params.stars, 'stars');
    this.addSimpleFilter(params.size, 'size');
    this.addSimpleFilter(params.created, 'created');

    if (params.updated) {
      this.queryParts.push(`pushed:${params.updated}`);
    }
    if (params.language) {
      this.queryParts.push(`language:${params.language}`);
    }

    const p = params as Record<string, unknown>;
    if (typeof p.forks === 'string') this.queryParts.push(`forks:${p.forks}`);
    if (typeof p.license === 'string')
      this.queryParts.push(`license:${p.license}`);
    if (typeof p.goodFirstIssues === 'string')
      this.queryParts.push(`good-first-issues:${p.goodFirstIssues}`);

    return this;
  }

  addMatchFilters(
    params: WithOptionalMeta<GitHubReposSearchSingleQuery>
  ): this {
    if (params.match) {
      const matches = Array.isArray(params.match)
        ? params.match
        : [params.match];
      matches.forEach(match => {
        if (match === 'name') {
          this.queryParts.push('in:name');
        } else if (match === 'description') {
          this.queryParts.push('in:description');
        } else if (match === 'readme') {
          this.queryParts.push('in:readme');
        }
      });
    }
    return this;
  }

  addQualityFilters(
    params?: WithOptionalMeta<GitHubReposSearchSingleQuery>
  ): this {
    const p = params as Record<string, unknown> | undefined;
    const archived = p?.archived as boolean | undefined;
    this.queryParts.push(
      archived === true ? 'archived:true' : 'is:not-archived'
    );
    const visibility = p?.visibility as string | undefined;
    if (visibility === 'public') this.queryParts.push('is:public');
    else if (visibility === 'private') this.queryParts.push('is:private');
    return this;
  }
}

export function buildCodeSearchQuery(
  params: WithOptionalMeta<GitHubCodeSearchQuery>
): string {
  return new CodeSearchQueryBuilder()
    .addQueryTerms(params)
    .addSearchFilters(params)
    .addOwnerRepo(params)
    .addMatchFilters(params)
    .build();
}

export function buildRepoSearchQuery(
  params: WithOptionalMeta<GitHubReposSearchSingleQuery>
): string {
  return new RepoSearchQueryBuilder()
    .addQueryTerms(params)
    .addOwnerRepo(params)
    .addRepoFilters(params)
    .addMatchFilters(params)
    .addQualityFilters(params)
    .build();
}
