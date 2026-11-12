// The GitHub search tools against known corpora (world-github.mjs). Truth reads the
// query fields directly; the loopback provider reads the qualifiers the tool sent.
import { CHANGE_STATUSES, ITEMS, REPOS, REPO_OWNER, SECTION_PR, itemHasTerm, repoMatches } from './world-github.mjs';
import { inventoryRows, rawPatch, structureFiles } from '../responses.mjs';
import { classificationBase, longContextProblem } from './classification-rows.mjs';
import { searchRegex } from './oracle-grep.mjs';

const rowsOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
const SORT_KEY = { stars: 'stars', forks: 'forks', helpWantedIssueCount: 'helpWanted', updatedAt: 'updated', pushedAt: 'pushed' };
const orderBy = (rows, value, order) => rows.toSorted((a, b) => (order === 'asc' ? 1 : -1) * (value(a) < value(b) ? -1 : value(a) > value(b) ? 1 : 0));
const distinct = values => values.filter((value, index) => values.indexOf(value) === index);

function repoTruth(ctx, { query }) {
  if (query.operation === 'get') return [[query.repo]];
  const archive = query.filters?.archive ?? 'exclude';
  const hits = REPOS.filter(repo => (archive === 'include' || (archive === 'only') === (repo.archived === true))
    && (query.operation === 'list' || repoMatches(repo, query.matchIn ?? []))
    && (!query.filters?.visibility || (repo.visibility ?? 'public') === query.filters.visibility)
    && (!query.filters?.isSponsorable || repo.sponsorable === true));
  return hits.map(repo => [repo.name]);
}

/** The listed order must follow a chosen key, walked over every page. */
function repoCheck(ctx, { query }, pages) {
  const key = SORT_KEY[query.sort?.by];
  if (!key) return null;
  const names = distinct(rowsOf(pages).flatMap(row => (row.repositories ?? []).map(repo => repo.repo)));
  const expected = orderBy(names.map(name => REPOS.find(repo => repo.name === name)), repo => repo[key], query.sort.order).map(repo => repo.name);
  return JSON.stringify(names) === JSON.stringify(expected) ? null : `${query.sort.by} ${query.sort.order}: ${names.join(', ')}`;
}

export const ghSearchRepo = {
  tool: 'ghSearchRepo',
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }],
  base: (row, assignment) => {
    if (row.operation === 'get') return { owner: REPO_OWNER, repo: 'by-text' };
    if (row.operation === 'list') return { owner: REPO_OWNER, sort: { by: 'pushedAt', order: 'desc' }, filters: { archive: 'include' }, ...(assignment.page === 'tiny' && { limit: 1 }) };
    return { owner: REPO_OWNER, keywords: ['alpha'], ...(assignment.page === 'tiny' && { limit: 1 }) };
  },
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 400; },
  truth: repoTruth,
  observed: (ctx, testCase, pages) => rowsOf(pages).flatMap(row => (row.repositories ?? []).map(repo => [repo.repo])),
  check: repoCheck,
  indistinct: { sort: 'order, judged by check against the corpus', operation: 'one value per form' },
};

const REVIEW = { changesRequested: 'changes_requested' };
const negated = field => `not${field[0].toUpperCase()}${field.slice(1)}`;
/** A scalar filter selects its value; its not<Field> (one value or a list) excludes each one. */
const selects = (filters, field, holds) => (filters[field] === undefined || holds(filters[field])) && ![filters[negated(field)] ?? []].flat().some(holds);

function historyTruth(ctx, { query }) {
  if (query.operation === 'commit') return ctx.mock.model.commits.filter(commit => !query.keywords || query.keywords.every(term => commit.message.toLowerCase().includes(term))).map(commit => [commit.oid]);
  const f = query.filters ?? {};
  const places = f.matchIn ?? [];
  const pull = query.operation === 'pullRequest';
  return ITEMS.filter(item => item.pull === pull
    && (query.keywords ?? []).every(term => itemHasTerm(item, term, places))
    && (!f.state || item.state === f.state)
    && (f.archive !== 'only')
    && (f.isMerged === undefined || Boolean(item.merged) === f.isMerged)
    && (f.isDraft === undefined || Boolean(item.draft) === f.isDraft)
    && selects(f, 'checkStatus', value => item.check === value)
    && selects(f, 'review', value => item.review === (REVIEW[value] ?? value))
    && selects(f, 'linked', () => item.linked)).map(item => [item.number]);
}

const HISTORY_ORDER = { commentsCount: item => item.comments.length, reactionCount: item => item.reactions, createdAt: item => item.day, updatedAt: item => item.updatedDay };
const historyRows = row => [...(row.pullRequests ?? []), ...(row.issues ?? [])];

function historyCheck(ctx, { query }, pages) {
  const key = HISTORY_ORDER[query.sort?.by];
  if (!key) return null;
  const numbers = distinct(rowsOf(pages).flatMap(row => historyRows(row).map(item => item.number)));
  const expected = orderBy(numbers.map(number => ITEMS.find(item => item.number === number)), key, query.sort.order).map(item => item.number);
  return JSON.stringify(numbers) === JSON.stringify(expected) ? null : `${query.sort.by} ${query.sort.order}: ${numbers.join(', ')}`;
}

export const ghSearchHistory = {
  tool: 'ghSearchHistory',
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }],
  base: (row, assignment) => {
    const limit = assignment.page === 'tiny' ? { limit: 1 } : {};
    if (row.operation === 'commit') return { owner: 'octo', repo: 'demo', ...limit, ...classificationBase(row, assignment) };
    return { owner: 'octo', repo: 'hist', ...(row.filters?.matchIn && { keywords: ['needle'] }), ...limit, ...classificationBase(row, assignment) };
  },
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 400; },
  truth: historyTruth,
  observed: (ctx, { query }, pages) => rowsOf(pages).flatMap(row => (query.operation === 'commit' ? (row.commits ?? []).map(commit => [commit.commitSha]) : historyRows(row).map(item => [item.number]))),
  check: (ctx, testCase, pages) => longContextProblem(ctx, testCase.query, historyTruth(ctx, testCase).length > 0) ?? historyCheck(ctx, testCase, pages),
  indistinct: { sort: 'order, judged by check against the corpus', operation: 'one value per form', 'classification.longContext': 'changes the provider request, judged by check; evidence is kept whole' },
};

const PATCH_LINES = [' Needle kept', '-needle old', '+needle new', ' Needlework'];

function itemBase(row) {
  if (row.operation === 'issue') return { owner: 'octo', repo: 'demo', number: 2 };
  if (row.operation === 'commit') return { owner: 'octo', repo: 'demo', ref: '<section-commit>', ...(row.matchString && {}) };
  if (row.operation === 'compare') return { owner: 'octo', repo: 'demo', baseRef: '<first-commit>', headRef: 'main' };
  return { owner: 'octo', repo: 'demo', number: SECTION_PR };
}

function itemPattern(row, assignment) {
  const searching = row.regexEngine || row.caseMode || row.wholeWord !== undefined;
  if (!searching) return {};
  const lower = (row.regexEngine ?? 'literal') === 'pcre2' ? 'needle(?! )' : 'needle';
  return { matchString: [assignment.casing === 'mixed' ? 'Needle' : lower], sections: ['patches'] };
}

function fileTruth(query) {
  const statuses = query.fileStatus ?? CHANGE_STATUSES;
  if (query.matchString && !patchTruth(query).length) return [];
  return statuses.filter(status => CHANGE_STATUSES.includes(status)).map(status => `changes/${status}.ts`);
}

function patchTruth(query) {
  if (!query.matchString) return PATCH_LINES;
  const regex = searchRegex(query);
  return PATCH_LINES.filter(line => [...line.slice(1).matchAll(regex)].length);
}

/** matchString also narrows the discussion sections to the text it finds. */
const NOT_NARROWED = new Set(['body', 'commits']);
const textPasses = (query, text) => !query.matchString || [...text.matchAll(searchRegex(query))].length > 0;

const issueTruth = query => [['body', !query.sections || query.sections.includes('body') ? 'The greeter needs a configurable prefix.' : null], ['comments', query.sections?.includes('comments') ? 1 : 0]];
const compareTruth = (ctx, query) => [['commits', (query.sections ?? ['files', 'commits']).includes('commits') ? ctx.mock.model.commits.length - 1 : 0]];
const SECTION_TEXT = [['body', 'Section body text'], ['comments', 'Section comment'], ['reviewComments', 'Section inline'], ['reviews', 'Section review'], ['commits', 'Section change']];

function pullTruth(query) {
  const sections = query.sections ?? [];
  const facts = SECTION_TEXT.filter(([section, value]) => sections.includes(section) && (NOT_NARROWED.has(section) || textPasses(query, value)));
  return sections.some(section => ['files', 'patches', 'commitFiles'].includes(section)) ? [...facts, ...fileTruth(query).map(file => ['file', file])] : facts;
}

function itemTruth(ctx, { query }) {
  if (query.operation === 'issue') return issueTruth(query);
  if (query.operation === 'compare') return compareTruth(ctx, query);
  // A commit's matchString filters its files as a pull request's does, patches section or not.
  if (query.operation === 'commit') return fileTruth(query).map(file => ['file', file]);
  return pullTruth(query);
}

const commitFileRows = commit => inventoryRows(commit.files ?? []).map(file => file.path ?? file);

function allPaths(item) {
  return [...inventoryRows(item.files ?? []).map(file => (typeof file === 'string' ? file : file.path)), ...(item.commits ?? []).flatMap(commitFileRows)];
}

function pullFacts(item, query) {
  const facts = item.body ? [['body', item.body]] : [];
  for (const section of ['comments', 'reviewComments', 'reviews']) for (const entry of item[section] ?? []) facts.push([section, entry.body]);
  if ((query.sections ?? []).includes('commits')) for (const commit of item.commits ?? []) facts.push(['commits', commit.messageHeadline]);
  return [...facts, ...allPaths(item).map(file => ['file', file])];
}

function issueFacts(data) {
  const issue = data[0]?.issues?.[0] ?? {};
  return [['body', issue.body ?? null], ['comments', (issue.comments ?? []).length]];
}

const commitFacts = data => data.flatMap(row => [...inventoryRows(row.files ?? []), ...(row.commits ?? []).flatMap(commit => inventoryRows(commit.files ?? []))]).map(file => ['file', file.path]);

function itemObserved(ctx, { query }, pages) {
  const data = rowsOf(pages);
  if (query.operation === 'issue') return issueFacts(data);
  if (query.operation === 'compare') return [['commits', data.flatMap(row => row.commits ?? []).length]];
  if (query.operation === 'commit') return commitFacts(data);
  return data.flatMap(row => row.pullRequests ?? []).flatMap(item => pullFacts(item, query));
}

function itemCheck(ctx, { query }, pages) {
  if (!query.matchString || !query.sections?.includes('patches') || query.operation === 'compare') return null;
  const files = rowsOf(pages).flatMap(row => [...(row.pullRequests ?? []), row]).flatMap(item => inventoryRows(item.files ?? []));
  const patch = files.find(file => file.patch)?.patch;
  const lines = patch ? rawPatch(patch).split('\n').slice(1).filter(Boolean) : [];
  const hits = patchTruth(query);
  if (!hits.length) return lines.length ? `no line can match, patch shows ${lines.join('|')}` : null;
  if (!files.some(file => file.patch)) return fileTruth(query).length ? `no patch for ${fileTruth(query).join(', ')}` : null;
  const missing = hits.filter(line => !lines.includes(line));
  return missing.length ? `patch misses ${missing.join(' | ')} of ${lines.join(' | ')}` : null;
}

/** Classification judges a pull request's or issue's discussion only: a read with none of it has nothing to judge. */
const DISCUSSION = new Set(['comments', 'reviews', 'reviewComments']);
const judgedDiscussion = (ctx, testCase) => itemTruth(ctx, testCase).some(([section, value]) => DISCUSSION.has(section) && Boolean(value));

export const ghGetHistoryItem = {
  tool: 'ghGetHistoryItem',
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }, { key: 'casing', values: ['lower', 'mixed'] }],
  base: (row, assignment) => ({ ...itemBase(row), ...itemPattern(row, assignment), ...classificationBase(row, assignment) }),
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 500; },
  resolve: (ctx, input) => JSON.parse(JSON.stringify(input).replaceAll('<section-commit>', ctx.sectionCommit).replaceAll('<first-commit>', ctx.mock.model.commits[0].oid)),
  truth: itemTruth,
  observed: itemObserved,
  check: (ctx, testCase, pages) => longContextProblem(ctx, testCase.query, judgedDiscussion(ctx, testCase)) ?? itemCheck(ctx, testCase, pages),
  indistinct: { operation: 'one value per form', caseMode: 'judged on patch text by check', regexEngine: 'judged on patch text by check', wholeWord: 'judged on patch text by check', 'classification.longContext': 'changes the provider request, judged by check; evidence is kept whole' },
};

const demoFiles = ctx => Object.keys(ctx.mock.model.files);

export const ghStructure = {
  tool: 'ghStructure',
  base: () => ({ owner: 'octo', repo: 'demo' }),
  shape: query => (query.operation === 'tree' ? { ...query, maxDepth: 4 } : query),
  truth: (ctx, { query }) => {
    if (query.operation === 'languages') return [['TypeScript'], ['Markdown']];
    if (query.operation === 'refs') return [['main'], ['v1.0.0']];
    return demoFiles(ctx).filter(file => query.materialize || file.split('/').length <= query.maxDepth).map(file => [file]);
  },
  observed: (ctx, { query }, pages) => {
    const data = rowsOf(pages);
    if (query.operation === 'languages') return Object.keys(data[0]?.languages ?? {}).map(name => [name]);
    if (query.operation === 'refs') return [...(data[0]?.branches ?? []).map(branch => branch.name).filter(name => name === 'main'), ...(data[0]?.tags ?? [])].map(name => [name]);
    return distinct(data.flatMap(row => structureFiles(row.files ?? [])).filter(entry => entry.type !== 'directory').map(entry => entry.path)).map(file => [file]);
  },
  check: (ctx, { query }, pages) => {
    if (!query.materialize) return null;
    const location = rowsOf(pages)[0]?.location;
    return location?.path ? null : `materialize true: no location: ${JSON.stringify(rowsOf(pages)[0]).slice(0, 200)}`;
  },
  indistinct: { operation: 'one value per form, each with its own oracle', materialize: 'adds a checkout, judged by check' },
};

const codeHits = (ctx, query) => {
  const only = query.matchIn;
  const inContent = !only || only.includes('content');
  const inPath = !only || only.includes('path');
  const files = ctx.mock.model.files;
  return Object.keys(files).filter(file => (inContent && files[file].toLowerCase().includes('greeter')) || (inPath && file.toLowerCase().includes('greeter')));
};

export const ghSearchCode = {
  tool: 'ghSearchCode',
  base: (row, assignment) => ({ owner: 'octo', repo: 'demo', keywords: ['greeter'], ...classificationBase(row, assignment) }),
  truth: (ctx, { query }) => (query.matchIn?.length === 1 && query.matchIn[0] === 'path' ? [['src/greeter.ts']] : codeHits(ctx, { ...query, matchIn: ['content'] }).map(file => [file])),
  observed: (ctx, testCase, pages) => distinct(rowsOf(pages).flatMap(row => (row.files ?? []).map(file => file.path))).map(file => [file]),
  check: (ctx, testCase) => longContextProblem(ctx, testCase.query),
  indistinct: { 'classification.longContext': 'changes the provider request, judged by check; evidence is kept whole' },
};
