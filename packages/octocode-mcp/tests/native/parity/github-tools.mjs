import assert from 'node:assert/strict';
import http from 'node:http';
import process from 'node:process';
import { Harness } from './harness.mjs';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer && addon && regexWorker,
  'usage: node github-tools.mjs <reference> <native> <addon> <regex-worker>',
);

const repository = {
  id: 1,
  node_id: 'R_fixture',
  name: 'repo',
  full_name: 'owner/repo',
  private: false,
  owner: { login: 'owner', id: 2, type: 'User' },
  html_url: 'https://github.com/owner/repo',
  description: 'parity fixture repository',
  fork: false,
  url: 'https://api.github.com/repos/owner/repo',
  created_at: '2020-01-01T00:00:00Z',
  updated_at: '2020-01-02T00:00:00Z',
  pushed_at: '2020-01-03T00:00:00Z',
  homepage: null,
  size: 12,
  stargazers_count: 7,
  watchers_count: 7,
  language: 'TypeScript',
  forks_count: 1,
  archived: false,
  disabled: false,
  open_issues_count: 0,
  license: { key: 'mit', name: 'MIT License', spdx_id: 'MIT' },
  topics: ['fixture'],
  visibility: 'public',
  forks: 1,
  open_issues: 0,
  watchers: 7,
  default_branch: 'main',
  score: 1,
};

const issue = {
  id: 3,
  node_id: 'I_fixture',
  number: 1,
  title: 'Parity issue',
  state: 'open',
  locked: false,
  user: { login: 'author', id: 4, type: 'User' },
  body: 'Issue body fixture.',
  html_url: 'https://github.com/owner/repo/issues/1',
  url: 'https://api.github.com/repos/owner/repo/issues/1',
  comments_url: 'https://api.github.com/repos/owner/repo/issues/1/comments',
  labels_url: 'https://api.github.com/repos/owner/repo/issues/1/labels{/name}',
  labels: [{ id: 5, name: 'bug', color: 'ff0000' }],
  assignees: [],
  milestone: null,
  comments: 0,
  created_at: '2020-02-01T00:00:00Z',
  updated_at: '2020-02-02T00:00:00Z',
  closed_at: null,
  author_association: 'OWNER',
};
const fileContent = '# Fixture\nHello from GitHub.\n';

const requests = [];
const server = http.createServer((request, response) => {
  const url = new URL(request.url ?? '/', 'http://127.0.0.1');
  requests.push(`${request.method} ${url.pathname}${url.search}`);
  response.setHeader('content-type', 'application/json');
  if (url.pathname === '/search/repositories') {
    response.end(JSON.stringify({ total_count: 1, incomplete_results: false, items: [repository] }));
    return;
  }
  if (url.pathname === '/repos/owner/repo') {
    response.end(JSON.stringify(repository));
    return;
  }
  if (url.pathname === '/repos/owner/repo/commits') {
    response.end(JSON.stringify([{
      sha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      commit: {
        author: { name: 'Fixture', email: 'fixture@example.test', date: '2020-01-03T00:00:00Z' },
        committer: { name: 'Fixture', email: 'fixture@example.test', date: '2020-01-03T00:00:00Z' },
        message: 'fixture commit',
      },
      author: { login: 'author' },
    }]));
    return;
  }
  if (url.pathname === '/repos/owner/repo/commits/main') {
    response.end(JSON.stringify({
      sha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      commit: {
        author: { name: 'Fixture', email: 'fixture@example.test', date: '2020-01-03T00:00:00Z' },
        committer: { name: 'Fixture', email: 'fixture@example.test', date: '2020-01-03T00:00:00Z' },
        message: 'fixture commit',
        tree: { sha: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' },
      },
    }));
    return;
  }
  if (url.pathname === '/repos/owner/repo/contents/README.md') {
    response.end(JSON.stringify({
      type: 'file',
      encoding: 'base64',
      size: Buffer.byteLength(fileContent),
      name: 'README.md',
      path: 'README.md',
      content: Buffer.from(fileContent).toString('base64'),
      sha: '0123456789012345678901234567890123456789',
      url: 'https://api.github.com/repos/owner/repo/contents/README.md',
      git_url: 'https://api.github.com/repos/owner/repo/git/blobs/fixture',
      html_url: 'https://github.com/owner/repo/blob/main/README.md',
      download_url: 'https://raw.githubusercontent.com/owner/repo/main/README.md',
    }));
    return;
  }
  if (url.pathname === '/search/issues') {
    response.end(JSON.stringify({ total_count: 1, incomplete_results: false, items: [issue] }));
    return;
  }
  if (url.pathname === '/repos/owner/repo/issues') {
    response.end(JSON.stringify([issue]));
    return;
  }
  if (url.pathname === '/repos/owner/repo/issues/1') {
    response.end(JSON.stringify(issue));
    return;
  }
  console.error(`unhandled fixture route: ${request.method} ${url.pathname}${url.search}`);
  response.writeHead(404);
  response.end(JSON.stringify({ message: `unhandled fixture route: ${url.pathname}` }));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const address = server.address();
assert.ok(address && typeof address === 'object');
const apiUrl = `http://127.0.0.1:${address.port}`;

const harness = await Harness.connect({
  reference: referenceServer,
  native: nativeServer,
  addon,
  regexWorker,
  extraEnv: {
    GITHUB_API_URL: apiUrl,
    OCTOCODE_GITHUB_GRAPHQL: 'false',
    GITHUB_TOKEN: 'fixture-token',
    GH_TOKEN: '',
  },
});
try {
  await harness.testTool('ghSearch', {
    queries: [{ operation: 'repositories', keywords: ['fixture'], pageSize: 5 }],
  });
  await harness.testTool('ghGetFileContent', {
    queries: [{
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      path: 'README.md',
      fullContent: true,
    }],
  });
  await harness.testTool('ghSearchHistory', {
    queries: [{ operation: 'issues', owner: 'owner', repo: 'repo', pageSize: 5 }],
  });
  await harness.testTool('ghGetHistoryItem', {
    queries: [{ operation: 'issue', owner: 'owner', repo: 'repo', number: 1 }],
  });
  for (const name of [
    'ghSearch',
    'ghGetFileContent',
    'ghSearchHistory',
    'ghGetHistoryItem',
  ]) {
    await harness.testTool(name, { queries: [{}] });
  }
  console.log(JSON.stringify({
    tools: ['ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem'],
    cases: 8,
    requests,
    status: 'partial',
  }));
} finally {
  await harness.close();
  await new Promise(resolve => server.close(resolve));
}
