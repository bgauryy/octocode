import type { z } from 'zod/v4';
import type {
  GitHubCodeSearchQuerySchema,
  GitHubReposSearchSingleQuerySchema,
} from '@octocodeai/octocode-core/schemas';

type GitHubCodeSearchQuery = z.infer<typeof GitHubCodeSearchQuerySchema>;
type GitHubReposSearchSingleQuery = z.infer<
  typeof GitHubReposSearchSingleQuerySchema
>;
import type { WithOptionalMeta } from '../types/execution.js';
import { GitHubPullRequestsSearchParams } from './githubAPI.js';

export function getOwnerQualifier(owner: string): string {
  return `user:${owner}`;
}

// Filter values (path:, filename:, …) only need quoting when they contain
// GitHub search separators such as `@` or `/` (e.g. path:"src/tools").
const GITHUB_SEARCH_SPECIAL_CHARS = /[@/]/;

// GitHub code search matches `path:` ONLY against a file's directory, never a
// full `dir/file.ext` — `path:packages/x/renderer.ts` returns zero even
// unquoted, while `filename:renderer.ts path:packages/x` works. So when a
// caller hands us a path whose last segment is a filename (a dot followed by a
// letter-led extension), we split it into those two qualifiers. The
// letter-led extension guard keeps version-like directory names (`src/v1.2`)
// from being mistaken for files.
const FILE_PATH_TAIL = /(?:^|\/)([^/]+\.[A-Za-z][A-Za-z0-9]{0,9})$/;

// A keyword is safe to send bare only if it is a single GitHub identifier
// token: alphanumerics plus `_`/`-`. Anything else — whitespace (a phrase),
// or syntax characters GitHub's search parser reacts to (`$ . ( ) [ ] { } @ /`
// `: " ' * ? + ^ | \` …) — must be wrapped in double quotes so it is matched as
// a literal phrase instead of being split into AND-ed tokens or swallowed by
// the query grammar. Without this, `path:`+multi-word queries and punctuation
// keywords (`$state`, `React.useState`) silently return zero results.
const GITHUB_BARE_KEYWORD = /^[A-Za-z0-9_-]+$/;

function quoteKeywordIfNeeded(keyword: string): string {
  if (keyword.startsWith('"')) {
    return keyword;
  }
  if (!GITHUB_BARE_KEYWORD.test(keyword)) {
    // Escape any embedded double quotes so the wrapper stays well-formed.
    return `"${keyword.replace(/"/g, '\\"')}"`;
  }
  return keyword;
}

abstract class BaseQueryBuilder {
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

  build(): string {
    return this.queryParts.join(' ').trim();
  }
}

class CodeSearchQueryBuilder extends BaseQueryBuilder {
  addQueryTerms(params: WithOptionalMeta<GitHubCodeSearchQuery>): this {
    if (
      Array.isArray(params.keywordsToSearch) &&
      params.keywordsToSearch.length > 0
    ) {
      const nonEmptyTerms = params.keywordsToSearch.filter(
        term => term && term.trim()
      );
      if (nonEmptyTerms.length > 0) {
        this.queryParts.push(...nonEmptyTerms.map(quoteKeywordIfNeeded));
      }
    }
    return this;
  }

  addSearchFilters(params: WithOptionalMeta<GitHubCodeSearchQuery>): this {
    let path = params.path;
    let filename = params.filename;
    // Rewrite a file-pointing path into filename: + directory path: (see
    // FILE_PATH_TAIL). Never clobber a filename the caller set explicitly.
    const fileTail =
      typeof path === 'string' && !filename ? path.match(FILE_PATH_TAIL) : null;
    if (fileTail) {
      filename = fileTail[1];
      // Everything before the matched "/basename.ext"; '' when the path was a
      // bare filename, in which case path: is dropped entirely.
      path = path!.slice(0, fileTail.index) || undefined;
    }

    this.addSimpleFilter(filename, 'filename');
    this.addSimpleFilter(params.extension, 'extension');
    this.addQuotedFilter(path, 'path');
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

class RepoSearchQueryBuilder extends BaseQueryBuilder {
  addQueryTerms(params: WithOptionalMeta<GitHubReposSearchSingleQuery>): this {
    if (
      Array.isArray(params.keywordsToSearch) &&
      params.keywordsToSearch.length > 0
    ) {
      this.queryParts.push(
        ...params.keywordsToSearch.map(quoteKeywordIfNeeded)
      );
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

    const language = (params as Record<string, unknown>).language;
    if (language && typeof language === 'string') {
      this.queryParts.push(`language:${language}`);
    }

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
    // Default (archived absent/false) keeps the historical `is:not-archived`
    // exclusion. `archived: true` opts INTO archived repos, which are
    // otherwise invisible to repo search.
    const archived = (params as { archived?: boolean } | undefined)?.archived;
    this.queryParts.push(
      archived === true ? 'archived:true' : 'is:not-archived'
    );
    return this;
  }
}

class PullRequestSearchQueryBuilder extends BaseQueryBuilder {
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
    this.addSimpleFilter(params.state, 'is');
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

  addEngagementFilters(params: GitHubPullRequestsSearchParams): this {
    this.addSimpleFilter(params.comments, 'comments');
    this.addSimpleFilter(params.reactions, 'reactions');
    this.addSimpleFilter(params.interactions, 'interactions');
    return this;
  }

  addReviewFilters(_params: GitHubPullRequestsSearchParams): this {
    return this;
  }

  addOrganizationFilters(params: GitHubPullRequestsSearchParams): this {
    this.addArrayFilter(params.label, 'label', true);
    return this;
  }

  addNegativeFilters(params: GitHubPullRequestsSearchParams): this {
    if (params['no-assignee']) this.queryParts.push('no:assignee');
    if (params['no-label']) this.queryParts.push('no:label');
    if (params['no-milestone']) this.queryParts.push('no:milestone');
    if (params['no-project']) this.queryParts.push('no:project');
    return this;
  }

  addMiscFilters(params: GitHubPullRequestsSearchParams): this {
    // Default (archived absent/false) excludes PRs in archived repos.
    // `archived: true` opts into them — needed for PR archaeology on
    // deprecated/archived projects.
    this.queryParts.push(
      params.archived === true ? 'archived:true' : 'archived:false'
    );
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
    params.query !== undefined ||
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
    params.created !== undefined ||
    params.updated !== undefined ||
    params['merged-at'] !== undefined ||
    params.closed !== undefined ||
    params.merged !== undefined ||
    Array.isArray(params.owner) ||
    Array.isArray(params.repo)
  );
}
