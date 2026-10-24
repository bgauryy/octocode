// GitHub REST payload shapes for the tool-health mock: repositories, commits,
// file changes, issues and pull requests built from the mock model, plus the
// search-query parser and Link-header pagination every list endpoint uses.
import { DATE, USER, addedLines, blobSha, sha, textOf } from './mock-model.mjs';

export function repoJson(base, repo) {
  const full = `${repo.owner}/${repo.name}`;
  return {
    id: Number.parseInt(sha(full).slice(0, 8), 16), node_id: `R_${repo.name}`, name: repo.name, full_name: full,
    owner: { ...USER, login: repo.owner }, private: false, html_url: `https://github.com/${full}`, description: repo.description,
    fork: false, url: `${base}/repos/${full}`, created_at: DATE(0), updated_at: DATE(3), pushed_at: DATE(3),
    homepage: null, size: 10, stargazers_count: repo.stars, watchers_count: repo.stars, language: repo.language,
    forks_count: 0, open_issues_count: 1, archived: repo.archived ?? false, disabled: false, visibility: 'public', default_branch: 'main',
    topics: ['greeter'], license: { key: 'mit', name: 'MIT License', spdx_id: 'MIT' }, has_issues: true,
  };
}

/** A search `q`: free terms (lowercased) and `name:value` qualifiers (quotes stripped). */
export function splitQuery(q) {
  const terms = [], qualifiers = {};
  for (const token of (q.match(/"[^"]*"|\S+/g) ?? [])) {
    const m = /^([a-z-]+):(.+)$/.exec(token);
    if (m) (qualifiers[m[1]] ??= []).push(m[2].replace(/^"|"$/g, ''));
    else terms.push(token.replace(/^"|"$/g, '').toLowerCase());
  }
  return { terms, qualifiers };
}

/** One `per_page`/`page` slice of `items` and its GitHub Link header. */
export function page(items, url, base) {
  const perPage = Math.max(1, Number(url.searchParams.get('per_page') ?? 30));
  const current = Math.max(1, Number(url.searchParams.get('page') ?? 1));
  const slice = items.slice((current - 1) * perPage, current * perPage);
  const last = Math.max(1, Math.ceil(items.length / perPage));
  const link = n => { const u = new URL(url.pathname + url.search, base); u.searchParams.set('page', String(n)); u.searchParams.set('per_page', String(perPage)); return u.href; };
  const rels = [];
  if (current < last) rels.push(`<${link(current + 1)}>; rel="next"`, `<${link(last)}>; rel="last"`);
  if (current > 1) rels.push(`<${link(1)}>; rel="first"`, `<${link(current - 1)}>; rel="prev"`);
  return { slice, headers: rels.length ? { link: rels.join(', ') } : {} };
}

/** One added file of a commit or pull request; `at` = { base, full, ref }. */
export function fileChange(at, p, text) {
  const { base, full, ref } = at;
  const added = addedLines(text);
  return {
    sha: blobSha(text), filename: p, status: 'added', additions: added.length, deletions: 0, changes: added.length,
    blob_url: `https://github.com/${full}/blob/${ref}/${p}`, raw_url: `https://github.com/${full}/raw/${ref}/${p}`,
    contents_url: `${base}/repos/${full}/contents/${p}?ref=${ref}`, patch: `@@ -0,0 +1,${added.length} @@\n${added.map(l => `+${l}`).join('\n')}`,
  };
}

export function commitJson(base, repo, commit, model) {
  const full = `${repo.owner}/${repo.name}`;
  const author = { name: 'Octo Dev', email: 'dev@example.invalid', date: commit.date };
  // Stats count every split line (a final newline adds one), unlike the per-file additions.
  const total = commit.added.reduce((n, p) => n + textOf(model, commit, p).split('\n').length, 0);
  return {
    sha: commit.oid, node_id: `C_${commit.oid.slice(0, 8)}`, url: `${base}/repos/${full}/commits/${commit.oid}`, html_url: `https://github.com/${full}/commit/${commit.oid}`,
    commit: { author, committer: author, message: commit.message, tree: { sha: commit.tree, url: `${base}/repos/${full}/git/trees/${commit.tree}` }, comment_count: 0 },
    author: USER, committer: USER, parents: commit.parent ? [{ sha: commit.parent, url: `${base}/repos/${full}/commits/${commit.parent}` }] : [],
    stats: { total, additions: total, deletions: 0 },
    files: commit.added.map(p => fileChange({ base, full, ref: commit.oid }, p, textOf(model, commit, p))),
  };
}

export function issueJson(base, repo, item, pull) {
  const full = `${repo.owner}/${repo.name}`;
  return {
    url: `${base}/repos/${full}/issues/${item.number}`, repository_url: `${base}/repos/${full}`, html_url: `https://github.com/${full}/${pull ? 'pull' : 'issues'}/${item.number}`,
    id: 1000 + item.number, node_id: `I_${item.number}`, number: item.number, title: item.title, user: USER, labels: [], state: item.state, locked: false,
    assignees: [], milestone: null, comments: item.comments?.length ?? 0, created_at: item.date, updated_at: item.date, closed_at: item.state === 'closed' ? item.date : null,
    author_association: 'OWNER', body: item.body, reactions: { total_count: 0 }, state_reason: null,
    ...(pull && { pull_request: { url: `${base}/repos/${full}/pulls/${item.number}`, html_url: `https://github.com/${full}/pull/${item.number}`, merged_at: item.merged ? item.date : null } }),
  };
}

export function pullJson(base, repo, pr, model) {
  const full = `${repo.owner}/${repo.name}`;
  const changed = pr.head.added;
  const adds = changed.reduce((n, p) => n + addedLines(textOf(model, pr.head, p)).length, 0);
  const ref = (label, commit) => ({ label: `${repo.owner}:${label}`, ref: label, sha: commit.oid, user: USER, repo: repoJson(base, repo) });
  const merged = pr.merged
    ? { merged_at: pr.date, merge_commit_sha: pr.head.oid, mergeable: null, merged_by: USER }
    : { merged_at: null, merge_commit_sha: null, mergeable: true, merged_by: null };
  return {
    ...issueJson(base, repo, pr, true), url: `${base}/repos/${full}/pulls/${pr.number}`, html_url: `https://github.com/${full}/pull/${pr.number}`,
    merged: pr.merged, merged_at: merged.merged_at, merge_commit_sha: merged.merge_commit_sha, draft: false, mergeable: merged.mergeable,
    head: ref(`feature-${pr.number}`, pr.head), base: ref('main', pr.base), commits: 1, additions: adds, deletions: 0, changed_files: changed.length,
    review_comments: pr.reviewComments?.length ?? 0, requested_reviewers: [], merged_by: merged.merged_by,
  };
}
