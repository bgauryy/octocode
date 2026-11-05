// Loopback GitHub corpora: repositories, history items and one pull request whose
// files carry every change status. Search answers follow GitHub's documented query
// language, so a filter that maps to the wrong qualifier selects the wrong rows.
import { PNG } from '../tool-health/classification-checks.mjs';
import { DATE, sha } from '../tool-health/mock-model.mjs';
import { commitJson, fileChange, issueJson, page, pullJson, repoJson, splitQuery } from '../tool-health/mock-github-json.mjs';

export const REPO_OWNER = 'rowner';
export const REPOS = [
  { name: 'alpha-name', description: 'plain', stars: 5, forks: 1, helpWanted: 3, updated: DATE(1), pushed: DATE(9) },
  { name: 'by-text', description: 'alpha text', stars: 9, forks: 7, helpWanted: 1, updated: DATE(4), pushed: DATE(7) },
  { name: 'by-readme', description: 'plain', readme: 'alpha readme', stars: 1, forks: 2, helpWanted: 2, updated: DATE(2), pushed: DATE(8) },
  { name: 'by-topic', description: 'plain', topics: ['alpha'], stars: 3, forks: 4, helpWanted: 5, updated: DATE(3), pushed: DATE(6) },
  { name: 'alpha-old', description: 'plain', archived: true, stars: 2, forks: 3, helpWanted: 4, updated: DATE(5), pushed: DATE(5) },
  { name: 'alpha-closed', description: 'plain', visibility: 'private', stars: 4, forks: 5, helpWanted: 6, updated: DATE(6), pushed: DATE(4) },
  { name: 'alpha-inner', description: 'plain', visibility: 'internal', stars: 6, forks: 6, helpWanted: 7, updated: DATE(7), pushed: DATE(3) },
  { name: 'alpha-sponsored', description: 'plain', sponsorable: true, stars: 7, forks: 8, helpWanted: 8, updated: DATE(8), pushed: DATE(2) },
];
const matchedIn = (repo, where) => ({ name: repo.name, description: repo.description, readme: repo.readme ?? '', topics: (repo.topics ?? []).join(' ') })[where].includes('alpha');
export const repoMatches = (repo, places) => (places.length ? places : ['name', 'description', 'topics']).some(where => matchedIn(repo, where));
const SORT_KEY = { stars: 'stars', forks: 'forks', 'help-wanted-issues': 'helpWanted', updated: 'updated', pushed: 'pushed' };

function repoRow(base, repo) {
  const json = repoJson(base, { owner: REPO_OWNER, stars: repo.stars, language: 'TypeScript', ...repo, files: {} });
  return { ...json, forks_count: repo.forks, updated_at: repo.updated, pushed_at: repo.pushed, visibility: repo.visibility ?? 'public', private: repo.visibility === 'private', topics: repo.topics ?? [], score: 1 };
}

function searchRepositories(request) {
  const { terms, qualifiers } = splitQuery(request.url.searchParams.get('q') ?? '');
  const places = (qualifiers.in ?? []).flatMap(value => value.split(','));
  const kinds = qualifiers.is ?? [];
  const hits = REPOS.filter(repo => (!terms.length || repoMatches(repo, places))
    && (!qualifiers.archived || qualifiers.archived.includes(String(repo.archived ?? false)))
    && kinds.every(kind => (kind === 'sponsorable' ? repo.sponsorable === true : (repo.visibility ?? 'public') === kind)));
  const key = SORT_KEY[request.url.searchParams.get('sort')];
  const order = request.url.searchParams.get('order') === 'asc' ? 1 : -1;
  const sorted = key ? hits.toSorted((a, b) => order * (a[key] < b[key] ? -1 : a[key] > b[key] ? 1 : 0)) : hits;
  const { slice, headers } = page(sorted, request.url, request.base);
  return { body: { total_count: sorted.length, incomplete_results: false, items: slice.map(repo => repoRow(request.base, repo)) }, headers };
}

function listRepositories(request) {
  const direction = request.url.searchParams.get('direction') === 'asc' ? 1 : -1;
  const sorted = REPOS.toSorted((a, b) => direction * a.pushed.localeCompare(b.pushed));
  const { slice, headers } = page(sorted, request.url, request.base);
  return { body: slice.map(repo => repoRow(request.base, repo)), headers };
}

/** Pull requests and issues of `octo/hist`, each differing on the fields search can filter. */
export const ITEMS = [
  { number: 11, pull: true, title: 'needle in title', body: 'plain', comments: [], state: 'open', draft: true, check: 'pending', review: 'none', linked: true, reactions: 1, day: 1, updatedDay: 9 },
  { number: 12, pull: true, title: 'plain', body: 'needle in body', comments: [], state: 'closed', merged: true, check: 'success', review: 'approved', linked: false, reactions: 5, day: 2, updatedDay: 8 },
  { number: 13, pull: true, title: 'plain', body: 'plain', comments: ['needle in comment'], state: 'open', check: 'failure', review: 'changes_requested', linked: true, reactions: 2, day: 3, updatedDay: 7 },
  { number: 14, pull: true, title: 'plain', body: 'plain', comments: ['one', 'two'], state: 'closed', check: 'success', review: 'required', linked: false, reactions: 3, day: 4, updatedDay: 6 },
  { number: 21, pull: false, title: 'needle issue', body: 'plain', comments: ['one', 'two', 'three'], state: 'open', linked: true, reactions: 4, day: 5, updatedDay: 5 },
  { number: 22, pull: false, title: 'plain', body: 'plain', comments: [], state: 'closed', linked: false, reactions: 0, day: 6, updatedDay: 4 },
];
const textIn = (item, where) => ({ title: item.title, body: item.body, comments: item.comments.join(' ') })[where];
export const itemHasTerm = (item, term, places) => (places.length ? places : ['title', 'body', 'comments']).some(where => textIn(item, where).toLowerCase().includes(term));
const SORT_VALUE = { comments: item => item.comments.length, reactions: item => item.reactions, created: item => item.day, updated: item => item.updatedDay };

const kindHolds = (item, kind) => ({
  pr: item.pull, issue: !item.pull, open: item.state === 'open', closed: item.state === 'closed',
  merged: item.merged === true, unmerged: item.merged !== true, draft: item.draft === true,
})[kind];

/** Whether an item passes one `name:value` qualifier; an unknown qualifier is a failure, not a pass. */
function qualifierHolds(item, name, value) {
  switch (name) {
    case 'repo': return value === 'octo/hist';
    case 'user': case 'org': return value === 'octo';
    case 'is': return kindHolds(item, value);
    case 'status': return item.check === value;
    case 'review': return item.review === value;
    case 'linked': return item.linked === true;
    case 'archived': return value === 'false';
    case 'in': return true;
    default: throw new Error(`world-github: no meaning for qualifier ${name}:${value}`);
  }
}

function searchItems(request) {
  const { terms, qualifiers } = splitQuery(request.url.searchParams.get('q') ?? '');
  if (!qualifiers.repo?.includes('octo/hist') && !(qualifiers.user ?? qualifiers.org)?.includes('octo')) return null;
  const places = (qualifiers.in ?? []).flatMap(value => value.split(','));
  const passes = item => Object.entries(qualifiers).every(([name, values]) => values.every(value => (name.startsWith('-') ? !qualifierHolds(item, name.slice(1), value) : qualifierHolds(item, name, value))));
  const hits = ITEMS.filter(item => terms.every(term => itemHasTerm(item, term, places)) && passes(item));
  const value = SORT_VALUE[request.url.searchParams.get('sort')];
  const order = request.url.searchParams.get('order') === 'asc' ? 1 : -1;
  const sorted = value ? hits.toSorted((a, b) => order * (value(a) - value(b))) : hits;
  const repo = { owner: 'octo', name: 'hist', language: 'TypeScript', stars: 1, files: {} };
  const { slice, headers } = page(sorted, request.url, request.base);
  const rows = slice.map(item => ({ ...issueJson(request.base, repo, { ...item, date: DATE(item.day), comments: item.comments }, item.pull), updated_at: DATE(item.updatedDay), draft: item.draft === true, score: 1 }));
  return { body: { total_count: sorted.length, incomplete_results: false, items: rows }, headers };
}

function listPulls(request) {
  const repo = { owner: 'octo', name: 'hist', language: 'TypeScript', stars: 1, files: {} };
  const state = request.url.searchParams.get('state') ?? 'open';
  const key = request.url.searchParams.get('sort') === 'updated' ? 'updatedDay' : 'day';
  const direction = request.url.searchParams.get('direction') === 'asc' ? 1 : -1;
  const pulls = ITEMS.filter(item => item.pull && (state === 'all' || item.state === state)).toSorted((a, b) => direction * (a[key] - b[key]));
  const head = request.model.head;
  const { slice, headers } = page(pulls, request.url, request.base);
  return { body: slice.map(item => ({ ...pullJson(request.base, repo, { ...item, date: DATE(item.day), base: head, head, merged: item.merged === true }, request.model), updated_at: DATE(item.updatedDay), created_at: DATE(item.day) })), headers };
}

/** The files of pull request 7 and of its commit: one per status GitHub reports. */
export const CHANGE_STATUSES = ['added', 'removed', 'modified', 'renamed', 'copied', 'changed', 'unchanged'];
const PATCH = ['@@ -1,3 +1,3 @@', ' Needle kept', '-needle old', '+needle new', ' Needlework'].join('\n');
export const SECTION_PR = 7;
const sectionCommitOid = sha('matrix section commit');
export const sectionFiles = (base, full, ref) => CHANGE_STATUSES.map(status => ({
  ...fileChange({ base, full, ref }, `changes/${status}.ts`, 'needle\n'), status, additions: 2, deletions: 1, changes: 3, patch: PATCH,
  ...(status === 'renamed' && { previous_filename: 'changes/was.ts' }),
}));

export function addSectionPullRequest(model) {
  const head = { oid: sectionCommitOid, message: 'Section change', date: DATE(4), get parent() { return model.head.oid; }, added: [], tree: sha('section tree'), files: {}, branch: `feature-${SECTION_PR}` };
  model.branchCommits.push(head);
  model.pulls.push({
    number: SECTION_PR, title: 'Section pull request', body: 'Section body text', state: 'open', merged: false, base: model.head, head, date: DATE(4),
    comments: [{ id: 70, body: 'Section comment', date: DATE(4) }],
    reviews: [{ id: 71, body: 'Section review', state: 'APPROVED', date: DATE(4) }],
    reviewComments: [{ id: 72, body: 'Section inline', date: DATE(4), path: 'changes/modified.ts', line: 2, side: 'RIGHT' }],
  });
  return sectionCommitOid;
}

function sectionFilesAnswer(request) {
  const files = sectionFiles(request.base, 'octo/demo', sectionCommitOid);
  if (request.p === `/repos/octo/demo/pulls/${SECTION_PR}/files`) { const { slice, headers } = page(files, request.url, request.base); return { body: slice, headers }; }
  const body = commitJson(request.base, { owner: 'octo', name: 'demo' }, request.model.branchCommits.find(commit => commit.oid === sectionCommitOid), request.model);
  return String(request.headers.accept ?? '').includes('vnd.github.sha') ? { body: sectionCommitOid } : { body: { ...body, files } };
}

function pixel() {
  return { body: { type: 'file', encoding: 'base64', path: 'pixel.png', size: PNG.length, sha: sha('pixel'), content: PNG.toString('base64') } };
}

function sectionComments(request) {
  const pull = request.model.pulls.find(item => item.number === SECTION_PR);
  return { body: pull.comments.map(comment => ({ id: comment.id, user: { login: 'octo-dev', type: 'User' }, body: comment.body, created_at: comment.date, updated_at: comment.date, html_url: 'https://github.com/octo/demo/pull/7', author_association: 'OWNER' })) };
}

const ROUTES = [
  [request => request.p === '/repos/octo/matrix/contents/pixel.png', pixel],
  [request => request.p === `/repos/octo/demo/issues/${SECTION_PR}/comments`, sectionComments],
  [request => request.p === '/search/repositories' && (request.url.searchParams.get('q') ?? '').includes(`user:${REPO_OWNER}`), searchRepositories],
  [request => [`/users/${REPO_OWNER}/repos`, `/orgs/${REPO_OWNER}/repos`].includes(request.p), listRepositories],
  [request => request.p === `/repos/${REPO_OWNER}` || request.p.startsWith(`/repos/${REPO_OWNER}/`), request => { const repo = REPOS.find(entry => request.p.endsWith(`/${entry.name}`)); return repo ? { body: repoRow(request.base, repo) } : { status: 404, body: { message: 'Not Found' } }; }],
  [request => request.p === '/search/issues', searchItems],
  [request => request.p === '/repos/octo/hist/pulls', listPulls],
  [request => request.p === `/repos/octo/demo/pulls/${SECTION_PR}/files` || request.p === `/repos/octo/demo/commits/${sectionCommitOid}`, sectionFilesAnswer],
];

/** The answer for a request the corpora own, else null so the shared mock answers. */
export function corpusAnswer(request) {
  for (const [owns, answer] of ROUTES) if (owns(request)) return answer(request);
  return null;
}
