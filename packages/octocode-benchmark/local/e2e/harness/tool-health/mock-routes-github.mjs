// GitHub REST routes of the tool-health mock: the three search endpoints and the
// `/repos/<owner>/<repo>/...` reads. Each route takes the request context `r`
// ({ url, p, headers, send, model, base }) and returns true when it answered.
import path from 'node:path';
import { USER, blobSha, commitByRef, filesAt, findRepo, sha, textOf } from './mock-model.mjs';
import { commitJson, fileChange, issueJson, page, pullJson, repoJson, splitQuery } from './mock-github-json.mjs';

/** Send one page of `items` with its Link header. */
function sendPage(r, items) {
  const { slice, headers } = page(items, r.url, r.base);
  r.send(200, slice, headers);
  return true;
}
const sendSearch = (r, hits, toItem = x => x) => {
  const { slice, headers } = page(hits, r.url, r.base);
  r.send(200, { total_count: hits.length, incomplete_results: false, items: slice.map(toItem) }, headers);
  return true;
};
const ownerAllowed = (qualifiers, owner) => (!qualifiers.user && !qualifiers.org) || [...(qualifiers.user ?? []), ...(qualifiers.org ?? [])].includes(owner);

/** Optional credential-scoped provider failure/partial response, never a query sentinel. */
function overridden(r) {
  const token = /^(?:token|bearer)\s+(.+)$/i.exec(r.headers.authorization ?? '')?.[1];
  const answer = r.model.githubResponders.get(token)?.(r);
  if (!answer) return false;
  r.send(answer.status ?? 200, answer.body, answer.headers);
  return true;
}
const scoped = route => r => overridden(r) || route(r);
const rangeMatches = (value, expression) => {
  const range = expression.split('..');
  if (range.length === 2) return Number(range[0]) <= value && value <= Number(range[1]);
  const match = /^(>=|<=|>|<)?(\d+)$/.exec(expression);
  if (!match) return false;
  const n = Number(match[2]);
  return ({ '>=': value >= n, '<=': value <= n, '>': value > n, '<': value < n })[match[1]] ?? value === n;
};
function searchRepositories(r) {
  const { terms, qualifiers: q } = splitQuery(r.url.searchParams.get('q') ?? '');
  let hits = r.model.repos.filter(repo => ownerAllowed(q, repo.owner)
    && (!q.repo || q.repo.includes(`${repo.owner}/${repo.name}`))
    && (!q.archived || q.archived.includes(String(repo.archived ?? false)))
    && (!q.language || q.language.some(language => language.toLowerCase() === repo.language.toLowerCase()))
    && (!q.stars || q.stars.every(range => rangeMatches(repo.stars, range)))
    && (!q.topic || q.topic.every(topic => topic === 'greeter'))
    && terms.every(t => `${repo.name} ${repo.description}`.toLowerCase().includes(t)));
  if (r.url.searchParams.get('sort') === 'stars') {
    const order = r.url.searchParams.get('order') === 'asc' ? 1 : -1;
    hits = hits.toSorted((a, b) => order * (a.stars - b.stars));
  }
  return sendSearch(r, hits, repo => ({ ...repoJson(r.base, repo), score: 1 }));
}

const pathAllowed = (qualifiers, fp) => (!qualifiers.path || qualifiers.path.some(prefix => fp.startsWith(prefix.replace(/^\//, ''))))
  && (!qualifiers.extension || qualifiers.extension.some(ext => fp.endsWith(`.${ext}`)))
  && (!qualifiers.filename || qualifiers.filename.some(name => path.basename(fp).includes(name)));

/** One code-search hit: the file with its first matching line as the text-match fragment. */
function codeHit(r, { repo, fp, text, terms, inPath }) {
  const full = `${repo.owner}/${repo.name}`;
  const head = r.model.head.oid;
  const fragment = text.split('\n').find(l => terms.some(t => l.toLowerCase().includes(t))) ?? '';
  const matches = terms.flatMap(t => { const i = fragment.toLowerCase().indexOf(t); return i >= 0 ? [{ text: fragment.slice(i, i + t.length), indices: [i, i + t.length] }] : []; });
  return {
    name: path.basename(fp), path: fp, sha: blobSha(text), url: `${r.base}/repos/${full}/contents/${fp}?ref=${head}`, git_url: `${r.base}/repos/${full}/git/blobs/${blobSha(text)}`,
    html_url: `https://github.com/${full}/blob/${head}/${fp}`, repository: repoJson(r.base, repo), score: 1,
    text_matches: inPath ? [] : [{ object_url: `${r.base}/repos/${full}/contents/${fp}`, object_type: 'FileContent', property: 'content', fragment, matches }],
  };
}

function searchCode(r) {
  const { terms, qualifiers } = splitQuery(r.url.searchParams.get('q') ?? '');
  const locations = (qualifiers.in ?? ['file']).flatMap(value => value.split(','));
  const inPath = locations.includes('path') && !locations.includes('file');
  const scope = r.model.repos.filter(repo => (qualifiers.repo ?? [`${repo.owner}/${repo.name}`]).includes(`${repo.owner}/${repo.name}`) && ownerAllowed(qualifiers, repo.owner));
  const hits = [];
  for (const repo of scope) for (const [fp, text] of Object.entries(repo.files)) {
    const hay = (inPath ? fp : text).toLowerCase();
    if (pathAllowed(qualifiers, fp) && terms.every(t => hay.includes(t))) hits.push(codeHit(r, { repo, fp, text, terms, inPath }));
  }
  return sendSearch(r, hits);
}

function issueScopeAllowed(qualifiers, item, pr) {
  return (!qualifiers.head || (pr && qualifiers.head.includes(`feature-${item.number}`)))
    && (!qualifiers.base || (pr && qualifiers.base.includes('main')))
    && (!qualifiers['-head'] || !qualifiers['-head'].includes(`feature-${item.number}`))
    && (!qualifiers.author || qualifiers.author.includes(USER.login))
    && (!qualifiers['-author'] || !qualifiers['-author'].includes(USER.login));
}

function searchIssues(r) {
  const { terms, qualifiers } = splitQuery(r.url.searchParams.get('q') ?? '');
  const { model } = r;
  const repo = qualifiers.repo?.[0] ? findRepo(model, ...qualifiers.repo[0].split('/')) : model.repos[0];
  if (!repo) { r.send(422, { message: 'Validation Failed', errors: [{ message: 'The listed users and repositories cannot be searched' }] }); return true; }
  const kinds = [...(qualifiers.is ?? []), ...(qualifiers.type ?? [])];
  const state = (qualifiers.is ?? []).find(v => v === 'open' || v === 'closed') ?? qualifiers.state?.[0];
  const pool = repo === model.repos[0] ? [...(kinds.includes('issue') ? [] : model.pulls.map(x => ({ x, pr: true }))), ...(kinds.includes('pr') ? [] : model.issues.map(x => ({ x, pr: false })))] : [];
  // As on GitHub, a commit SHA term finds the pull requests that contain it (here: the head commit).
  const hits = pool.filter(({ x, pr }) => (!state || x.state === state)
    && (!kinds.includes('merged') || x.merged === true) && (!kinds.includes('unmerged') || x.merged !== true)
    && issueScopeAllowed(qualifiers, x, pr)
    && terms.every(t => `${x.title} ${x.body} ${pr ? x.head.oid : ''}`.toLowerCase().includes(t)))
    .toSorted((a, b) => (r.url.searchParams.get('order') === 'asc' ? 1 : -1) * (a.x.number - b.x.number));
  return sendSearch(r, hits, ({ x, pr }) => ({ ...issueJson(r.base, repo, x, pr), score: 1 }));
}

function searchCommits(r) {
  const { terms, qualifiers } = splitQuery(r.url.searchParams.get('q') ?? '');
  const repo = qualifiers.repo?.[0] ? findRepo(r.model, ...qualifiers.repo[0].split('/')) : r.model.repos[0];
  const hits = repo === r.model.repos[0] ? r.model.commits.filter(c => terms.every(t => c.message.toLowerCase().includes(t))) : [];
  return sendSearch(r, hits, c => ({ ...commitJson(r.base, repo, c, r.model), repository: repoJson(r.base, repo) }));
}
export const SEARCH_ROUTES = Object.fromEntries(Object.entries({ '/search/repositories': searchRepositories, '/search/code': searchCode, '/search/issues': searchIssues, '/search/commits': searchCommits }).map(([key, route]) => [key, scoped(route)]));

// ---- /repos/<owner>/<repo>/<rest>: `r` also carries `repo` and `full`.
function commitRoute(r, m) {
  const commit = commitByRef(r.model, m[1]);
  if (!commit) r.send(422, { message: `No commit found for SHA: ${m[1]}` });
  else if (String(r.headers.accept ?? '').includes('vnd.github.sha')) r.send(200, commit.oid);
  else {
    const body = commitJson(r.base, r.repo, commit, r.model);
    const { slice, headers } = page(body.files, r.url, r.base);
    r.send(200, { ...body, files: slice }, headers);
  }
  return true;
}

function commitsRoute(r) {
  const filePath = r.url.searchParams.get('path');
  const start = commitByRef(r.model, r.url.searchParams.get('sha') ?? 'HEAD');
  const chain = [];
  for (let c = start; c; c = c.parent ? commitByRef(r.model, c.parent) : null) {
    if ((!filePath || c.added.some(a => a === filePath || a.startsWith(`${filePath}/`)))
      && (!r.url.searchParams.has('since') || c.date >= r.url.searchParams.get('since'))
      && (!r.url.searchParams.has('until') || c.date <= r.url.searchParams.get('until'))) chain.push(c);
  }
  const { slice, headers } = page(chain, r.url, r.base);
  r.send(200, slice.map(c => commitJson(r.base, r.repo, c, r.model)), headers);
  return true;
}

function compareRoute(r, m) {
  const { model, base, repo, full } = r;
  const from = commitByRef(model, m[1]), to = commitByRef(model, m[2]);
  if (!from || !to) { r.send(404, { message: 'Not Found' }); return true; }
  const between = model.commits.slice(model.commits.indexOf(from) + 1, model.commits.indexOf(to) + 1);
  r.send(200, {
    url: `${base}/repos/${full}/compare/${m[1]}...${m[2]}`, html_url: `https://github.com/${full}/compare/${m[1]}...${m[2]}`,
    status: 'ahead', ahead_by: between.length, behind_by: 0, total_commits: between.length,
    base_commit: commitJson(base, repo, from, model), merge_base_commit: commitJson(base, repo, from, model), commits: between.map(c => commitJson(base, repo, c, model)),
    files: between.flatMap(c => c.added.map(fp => fileChange({ base, full, ref: to.oid }, fp, textOf(model, c, fp)))),
  });
  return true;
}

function fileContent(r, { commit, target, text }) {
  const { base, full } = r;
  if (String(r.headers.accept ?? '').includes('raw')) { r.send(200, text, { 'content-type': 'application/vnd.github.raw; charset=utf-8' }); return true; }
  r.send(200, {
    type: 'file', encoding: 'base64', size: Buffer.byteLength(text), name: path.basename(target), path: target, content: Buffer.from(text).toString('base64'), sha: blobSha(text),
    url: `${base}/repos/${full}/contents/${target}?ref=${commit.oid}`, html_url: `https://github.com/${full}/blob/${commit.oid}/${target}`, download_url: null, git_url: `${base}/repos/${full}/git/blobs/${blobSha(text)}`,
  });
  return true;
}

function contentsRoute(r, m) {
  const commit = commitByRef(r.model, r.url.searchParams.get('ref') ?? 'HEAD');
  if (!commit) { r.send(404, { message: 'No commit found for the ref' }); return true; }
  const visible = filesAt(r.model, r.repo, commit);
  const target = m[1] ?? '';
  if (visible[target] !== undefined) return fileContent(r, { commit, target, text: visible[target] });
  const prefix = target ? `${target}/` : '';
  const children = new Map();
  for (const fp of Object.keys(visible)) if (fp.startsWith(prefix)) children.set(fp.slice(prefix.length).split('/')[0], fp.slice(prefix.length).includes('/') ? 'dir' : 'file');
  if (!children.size) { r.send(404, { message: 'Not Found' }); return true; }
  const { base, full } = r;
  r.send(200, [...children].map(([name, type]) => ({
    type, name, path: prefix + name, size: type === 'file' ? Buffer.byteLength(visible[prefix + name]) : 0, sha: type === 'file' ? blobSha(visible[prefix + name]) : sha(`dir ${prefix + name}`),
    url: `${base}/repos/${full}/contents/${prefix + name}`, html_url: `https://github.com/${full}/tree/${commit.oid}/${prefix + name}`, download_url: null,
  })));
  return true;
}

function treeRoute(r, m) {
  const { model, base, full } = r;
  const commit = [...model.commits, ...model.branchCommits].find(c => c.tree === m[1]) ?? commitByRef(model, m[1]);
  if (!commit) { r.send(404, { message: 'Not Found' }); return true; }
  const visible = filesAt(model, r.repo, commit);
  const dirs = new Set();
  for (const fp of Object.keys(visible)) { const parts = fp.split('/'); for (let i = 1; i < parts.length; i++) dirs.add(parts.slice(0, i).join('/')); }
  const tree = [
    ...[...dirs].map(d => ({ path: d, mode: '040000', type: 'tree', sha: sha(`dir ${d}`), url: `${base}/repos/${full}/git/trees/${sha(`dir ${d}`)}` })),
    ...Object.entries(visible).map(([fp, text]) => ({ path: fp, mode: '100644', type: 'blob', sha: blobSha(text), size: Buffer.byteLength(text), url: `${base}/repos/${full}/git/blobs/${blobSha(text)}` })),
  ].toSorted((a, b) => a.path.localeCompare(b.path));
  r.send(200, { sha: commit.tree, url: `${base}/repos/${full}/git/trees/${commit.tree}`, tree, truncated: false });
  return true;
}

/** main, every pull request head branch, and one release tag on main's head. */
function refsRoute(r, m) {
  const { model, base, full } = r;
  const tags = m[1] === 'tags';
  const refs = tags
    ? [{ name: 'v1.0.0', commit: model.head }]
    : [{ name: 'main', commit: model.head }, ...(r.repo === model.repos[0] ? model.pulls.map(pr => ({ name: `feature-${pr.number}`, commit: pr.head })) : [])];
  const extra = name => (tags ? { zipball_url: `${base}/repos/${full}/zipball/${name}`, tarball_url: `${base}/repos/${full}/tarball/${name}`, node_id: `T_${name}` } : { protected: false });
  return sendPage(r, refs.map(ref => ({ name: ref.name, commit: { sha: ref.commit.oid, url: `${base}/repos/${full}/commits/${ref.commit.oid}` }, ...extra(ref.name) })));
}

function matchingTags(r) {
  const commit = r.model.head;
  return sendPage(r, [{
    ref: 'refs/tags/v1.0.0',
    object: { type: 'commit', sha: commit.oid, url: `${r.base}/repos/${r.full}/git/commits/${commit.oid}` },
  }]);
}

function pullsRoute(r) {
  const params = r.url.searchParams;
  const state = params.get('state') ?? 'open';
  const head = params.get('head');
  const baseRef = params.get('base');
  const sort = params.get('sort') === 'updated' ? 'updated_at' : 'created_at';
  const direction = params.get('direction') === 'asc' ? 1 : -1;
  const pulls = (r.repo === r.model.repos[0] ? r.model.pulls : [])
    .map(pr => pullJson(r.base, r.repo, pr, r.model))
    .filter(pr => (state === 'all' || pr.state === state) && (!head || head === pr.head.label || head === pr.head.ref) && (!baseRef || baseRef === pr.base.ref))
    .toSorted((a, b) => direction * (a[sort].localeCompare(b[sort]) || a.number - b.number));
  return sendPage(r, pulls);
}

function pullRequestItems(r, pr, sub) {
  const { model, base, full } = r;
  if (sub === 'files') return pr.head.added.map(fp => fileChange({ base, full, ref: pr.head.oid }, fp, textOf(model, pr.head, fp)));
  if (sub === 'commits') return [commitJson(base, r.repo, pr.head, model)];
  if (sub === 'reviews') return (pr.reviews ?? []).map(review => ({ id: review.id, body: review.body, state: review.state, submitted_at: review.date, user: USER, commit_id: pr.head.oid }));
  if (sub === 'comments') return (pr.reviewComments ?? []).map(comment => ({ ...comment, user: USER, created_at: comment.date, updated_at: comment.date, commit_id: pr.head.oid }));
  return null;
}

/** A pull request's or issue's sub-collection; false when it has none. */
function itemList(r, { pr, issue, number, sub, collection }) {
  if (pr && collection === 'pulls') {
    const items = pullRequestItems(r, pr, sub);
    if (items) return sendPage(r, items);
  }
  if (sub === 'comments') return sendPage(r, (issue?.comments ?? []).map(c => ({ id: c.id, user: USER, body: c.body, created_at: c.date, updated_at: c.date, html_url: `https://github.com/${r.full}/issues/${number}#issuecomment-${c.id}`, author_association: 'OWNER' })));
  if (['reviews', 'events', 'timeline'].includes(sub)) return sendPage(r, []);
  return false;
}

function itemRoute(r, m) {
  const number = Number(m[2]);
  const demo = r.repo === r.model.repos[0];
  const pr = demo && r.model.pulls.find(x => x.number === number);
  const issue = demo && r.model.issues.find(x => x.number === number);
  if (!pr && (m[1] === 'pulls' || !issue)) { r.send(404, { message: 'Not Found' }); return true; }
  if (m[3]) return itemList(r, { pr, issue, number, sub: m[3], collection: m[1] });
  r.send(200, m[1] === 'pulls' ? pullJson(r.base, r.repo, pr, r.model) : issueJson(r.base, r.repo, pr || issue, !!pr));
  return true;
}

const REPO_ROUTES = [
  [/^$/, r => { r.send(200, repoJson(r.base, r.repo)); return true; }],
  [/^languages$/, r => { r.send(200, r.repo.languages ?? { [r.repo.language]: Object.values(r.repo.files).reduce((n, text) => n + Buffer.byteLength(text), 0) }); return true; }],
  [/^commits\/(.+)$/, commitRoute],
  [/^commits$/, commitsRoute],
  [/^compare\/(.+?)\.\.\.(.+)$/, compareRoute],
  [/^contents(?:\/(.*))?$/, contentsRoute],
  [/^git\/trees\/(.+)$/, treeRoute],
  [/^git\/matching-refs\/tags$/, matchingTags],
  [/^(branches|tags)$/, refsRoute],
  [/^pulls$/, pullsRoute],
  [/^(pulls|issues)\/(\d+)(?:\/(\w+))?$/, itemRoute],
];

/** `/repos/<owner>/<repo>[/<rest>]`: false for any other path or an unknown read. */
export function repoRoute(r) {
  if (overridden(r)) return true;
  const owner = /^\/(?:users|orgs)\/([^/]+)\/repos$/.exec(r.p);
  if (owner) return sendPage(r, r.model.repos.filter(repo => repo.owner === owner[1]).map(repo => repoJson(r.base, repo)));
  const m = /^\/repos\/([^/]+)\/([^/]+)(?:\/(.*))?$/.exec(r.p);
  if (!m) return false;
  const repo = findRepo(r.model, m[1], m[2]);
  if (!repo) { r.send(404, { message: 'Not Found', documentation_url: 'https://docs.github.com/rest' }); return true; }
  const rest = m[3] ?? '';
  for (const [pattern, handle] of REPO_ROUTES) {
    const match = pattern.exec(rest);
    if (match) return handle({ ...r, repo, full: `${repo.owner}/${repo.name}` }, match);
  }
  return false;
}
