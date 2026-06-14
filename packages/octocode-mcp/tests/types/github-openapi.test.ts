import { describe, it, expect } from 'vitest';
import {
  isGitHubAPIError,
  isGitHubAPISuccess,
  isRepository,
  type GitHubAPIError,
  type GitHubAPISuccess,
  type Repository,
} from '../../../octocode-tools-core/src/github/githubAPI';

function createUser(login: string, id: number) {
  return {
    login,
    id,
    node_id: `MDQ6VXNlciR7aWR9`,
    avatar_url: `https://github.com/images/error/octocat_happy.gif`,
    gravatar_id: '',
    url: `https://api.github.com/users/${login}`,
    html_url: `https://github.com/${login}`,
    followers_url: `https://api.github.com/users/${login}/followers`,
    following_url: `https://api.github.com/users/${login}/following{/other_user}`,
    gists_url: `https://api.github.com/users/${login}/gists{/gist_id}`,
    starred_url: `https://api.github.com/users/${login}/starred{/owner}{/repo}`,
    subscriptions_url: `https://api.github.com/users/${login}/subscriptions`,
    organizations_url: `https://api.github.com/users/${login}/orgs`,
    repos_url: `https://api.github.com/users/${login}/repos`,
    events_url: `https://api.github.com/users/${login}/events{/privacy}`,
    received_events_url: `https://api.github.com/users/${login}/received_events`,
    type: 'User' as const,
    site_admin: false,
  };
}

function createMinimalRepository(
  id: number,
  name: string,
  fullName: string,
  isPrivate: boolean
): Repository {
  const owner = createUser(fullName.split('/')[0] || 'unknown', id + 1000);
  return {
    id,
    node_id: `MDEwOlJlcG9zaXRvcnkke2lkfQ==`,
    name,
    full_name: fullName,
    private: isPrivate,
    owner,
    html_url: `https://github.com/${fullName}`,
    description: `Test repository ${name}`,
    fork: false,
    url: `https://api.github.com/repos/${fullName}`,
    forks_url: `https://api.github.com/repos/${fullName}/forks`,
    keys_url: `https://api.github.com/repos/${fullName}/keys{/key_id}`,
    collaborators_url: `https://api.github.com/repos/${fullName}/collaborators{/collaborator}`,
    teams_url: `https://api.github.com/repos/${fullName}/teams`,
    hooks_url: `https://api.github.com/repos/${fullName}/hooks`,
    issue_events_url: `https://api.github.com/repos/${fullName}/issues/events{/number}`,
    events_url: `https://api.github.com/repos/${fullName}/events`,
    assignees_url: `https://api.github.com/repos/${fullName}/assignees{/user}`,
    branches_url: `https://api.github.com/repos/${fullName}/branches{/branch}`,
    tags_url: `https://api.github.com/repos/${fullName}/tags`,
    blobs_url: `https://api.github.com/repos/${fullName}/git/blobs{/sha}`,
    git_tags_url: `https://api.github.com/repos/${fullName}/git/tags{/sha}`,
    git_refs_url: `https://api.github.com/repos/${fullName}/git/refs{/sha}`,
    trees_url: `https://api.github.com/repos/${fullName}/git/trees{/sha}`,
    statuses_url: `https://api.github.com/repos/${fullName}/statuses/{sha}`,
    languages_url: `https://api.github.com/repos/${fullName}/languages`,
    stargazers_url: `https://api.github.com/repos/${fullName}/stargazers`,
    contributors_url: `https://api.github.com/repos/${fullName}/contributors`,
    subscribers_url: `https://api.github.com/repos/${fullName}/subscribers`,
    subscription_url: `https://api.github.com/repos/${fullName}/subscription`,
    commits_url: `https://api.github.com/repos/${fullName}/commits{/sha}`,
    git_commits_url: `https://api.github.com/repos/${fullName}/git/commits{/sha}`,
    comments_url: `https://api.github.com/repos/${fullName}/comments{/number}`,
    issue_comment_url: `https://api.github.com/repos/${fullName}/issues/comments{/number}`,
    contents_url: `https://api.github.com/repos/${fullName}/contents/{+path}`,
    compare_url: `https://api.github.com/repos/${fullName}/compare/{base}...{head}`,
    merges_url: `https://api.github.com/repos/${fullName}/merges`,
    archive_url: `https://api.github.com/repos/${fullName}/{archive_format}{/ref}`,
    downloads_url: `https://api.github.com/repos/${fullName}/downloads`,
    issues_url: `https://api.github.com/repos/${fullName}/issues{/number}`,
    pulls_url: `https://api.github.com/repos/${fullName}/pulls{/number}`,
    milestones_url: `https://api.github.com/repos/${fullName}/milestones{/number}`,
    notifications_url: `https://api.github.com/repos/${fullName}/notifications{?since,all,participating}`,
    labels_url: `https://api.github.com/repos/${fullName}/labels{/name}`,
    releases_url: `https://api.github.com/repos/${fullName}/releases{/id}`,
    deployments_url: `https://api.github.com/repos/${fullName}/deployments`,
    created_at: '2023-01-01T00:00:00Z',
    updated_at: '2023-01-01T00:00:00Z',
    pushed_at: '2023-01-01T00:00:00Z',
    git_url: `git://github.com/${fullName}.git`,
    ssh_url: `git@github.com:${fullName}.git`,
    clone_url: `https://github.com/${fullName}.git`,
    svn_url: `https://github.com/${fullName}`,
    homepage: null,
    size: 100,
    stargazers_count: 0,
    watchers_count: 0,
    language: 'JavaScript',
    has_issues: true,
    has_projects: true,
    has_wiki: true,
    has_pages: false,
    has_downloads: true,
    archived: false,
    disabled: false,
    open_issues_count: 0,
    license: null,
    allow_forking: true,
    is_template: false,
    web_commit_signoff_required: false,
    topics: [],
    visibility: isPrivate ? 'private' : 'public',
    forks: 0,
    open_issues: 0,
    watchers: 0,
    default_branch: 'main',
    permissions: {
      admin: false,
      maintain: false,
      push: false,
      triage: false,
      pull: true,
    },
    temp_clone_token: '',
    allow_squash_merge: true,
    allow_merge_commit: true,
    allow_rebase_merge: true,
    allow_auto_merge: false,
    delete_branch_on_merge: false,
    allow_update_branch: false,
    use_squash_pr_title_as_default: false,
    squash_merge_commit_message: 'COMMIT_MESSAGES',
    squash_merge_commit_title: 'COMMIT_OR_PR_TITLE',
    merge_commit_message: 'PR_TITLE',
    merge_commit_title: 'MERGE_MESSAGE',
    network_count: 0,
    subscribers_count: 0,
    mirror_url: null,
    forks_count: 0,
    has_discussions: false,
  };
}

describe('GitHub OpenAPI Type Guards', () => {
  describe('isGitHubAPIError', () => {
    it('should return true for valid error objects', () => {
      const error: GitHubAPIError = {
        error: 'Not found',
        status: 404,
        type: 'http',
      };

      expect(isGitHubAPIError(error)).toBe(true);
    });

    it('should return false for non-error objects', () => {
      expect(isGitHubAPIError({ data: 'test' })).toBe(false);
      expect(isGitHubAPIError({ error: 123 })).toBe(false);
      expect(isGitHubAPIError(null)).toBe(false);
      expect(isGitHubAPIError(undefined)).toBe(false);
      expect(isGitHubAPIError('string')).toBe(false);
      expect(isGitHubAPIError(123)).toBe(false);
    });
  });

  describe('isGitHubAPISuccess', () => {
    it('should return true for valid success objects', () => {
      const success: GitHubAPISuccess<string> = {
        data: 'test data',
        status: 200,
      };

      expect(isGitHubAPISuccess(success)).toBe(true);
    });

    it('should return false for non-success objects', () => {
      expect(isGitHubAPISuccess({ error: 'test' })).toBe(false);
      expect(isGitHubAPISuccess({ data: 'test' })).toBe(false);
      expect(isGitHubAPISuccess({ status: 200 })).toBe(false);
      expect(isGitHubAPISuccess(null)).toBe(false);
      expect(isGitHubAPISuccess(undefined)).toBe(false);
      expect(isGitHubAPISuccess('string')).toBe(false);
      expect(isGitHubAPISuccess(123)).toBe(false);
    });
  });

  describe('isRepository', () => {
    it('should return true for valid repository objects', () => {
      const repo = createMinimalRepository(
        123,
        'test-repo',
        'owner/test-repo',
        false
      );
      expect(isRepository(repo)).toBe(true);
    });

    it('should return false for non-repository objects', () => {
      expect(isRepository({ id: 123 })).toBe(false);
      expect(isRepository({ name: 'test' })).toBe(false);
      expect(isRepository(null)).toBe(false);
      expect(isRepository(undefined)).toBe(false);
      expect(isRepository('string')).toBe(false);
      expect(isRepository(123)).toBe(false);
    });
  });
});
