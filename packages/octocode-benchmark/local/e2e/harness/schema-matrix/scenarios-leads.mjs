// Lead channels and the partial reasons that come with provider shapes: each scene
// gives the provider the answer that makes a tool offer the lead, then the driver
// follows it verbatim to its end.
import { classificationAnswer } from '../tool-health/mock-routes.mjs';
import { LSP_SCENARIOS } from './scenarios-lsp.mjs';

const R = { owner: 'octo', repo: 'demo' };
const PR = { ...R, operation: 'pullRequest', number: 7 };
const file = (name, more = {}) => ({ sha: 'c'.repeat(40), filename: name, status: 'modified', additions: 1, deletions: 1, changes: 2, ...more });
const pullFiles = files => request => (request.p === '/repos/octo/demo/pulls/7/files' ? { body: files } : null);
const scene = (id, tool) => (query, expects, more = {}) => ({ id: `lead.${id}`, tool, query, ...expects, ...more });
const score = (question, decision) => {
  const levels = question.criteria ?? ['low', 'mid', 'high'];
  const probabilities = Object.fromEntries(levels.map((_, level) => [String(level), decision === 'unknown' ? (level < 2 ? 0.4 : 0.2) : Number(level === (decision === 'selected' ? levels.length - 1 : 0))]));
  const expected = Object.entries(probabilities).reduce((sum, [level, p]) => sum + Number(level) * p, 0);
  return { type: 'score', score: Number(expected.toFixed(4)), confidence: decision === 'unknown' ? 0.4 : 1, probabilities, legend: Object.fromEntries(levels.map((label, level) => [String(level), label])) };
};
const answering = decide => request => ({ body: classificationAnswer(request, (question, id) => score(question, decide(request, id))) });
const predicates = (id, more = {}) => ({ classification: { predicates: [`scenario ${id}`], ...more } });
const MANY_LINES = Array.from({ length: 700 }, (_, index) => `${index < 350 ? 'selected' : 'unknown'} evidence line ${index} that a judge reads for the selection walk`).join('\n');

const mixedRead = answering(request => JSON.stringify(request.state).includes('unknown evidence line') ? 'unknown' : 'selected');

const HISTORY = [
  scene('continueCommitFiles', 'ghGetHistoryItem')({ ...PR, sections: ['commitFiles'] }, { channels: ['continueCommitFiles'] }, {
    github: request => (/\/commits\/[0-9a-f]{40}$/.test(request.p) && request.p.startsWith('/repos/octo/demo/') && !(request.url.searchParams.get('page') > 1)
      ? { body: { sha: request.p.split('/').pop(), commit: { message: 'm', author: { name: 'n', date: '2025-01-01T00:00:00Z' } }, parents: [{ sha: 'd'.repeat(40) }], files: Array.from({ length: 300 }, (_, index) => file(`bulk/f${index}.ts`, { patch: '@@ -1 +1 @@\n+x' })) }, headers: { link: `<${request.base}${request.p}?page=2&per_page=300>; rel="next"` } } : null),
    allow: ['lead.execution'],
  }),
  scene('expandContext', 'ghGetHistoryItem')({ ...PR, sections: ['patches'], matchString: ['needle'], contextLines: { before: 10, after: 10 } }, { channels: ['expandContext'], reasons: ['contextClipped'] }, {
    github: request => pullFiles([file('src/short.ts', { patch: '@@ -50,3 +50,3 @@\n a\n-needle old\n+needle new\n b' })])(request) ?? (request.p.startsWith('/repos/octo/demo/contents/src/short.ts') ? { status: 500, body: { message: 'unreadable' } } : null),
    allow: ['lead.execution'],
  }),
  scene('readAtCommit.readParent', 'ghGetHistoryItem')({ ...PR, sections: ['patches'] }, { channels: ['readAtCommit', 'readParent'] }, { github: pullFiles([file('src/huge.rs')]), allow: ['lead.execution'] }),
  scene('searchUnpatchedFile', 'ghGetHistoryItem')({ ...PR, sections: ['patches'], matchString: ['needle'] }, { channels: ['searchUnpatchedFile'], reasons: ['patchUnavailable'] }, { github: pullFiles([file('src/nopatch.rs')]), allow: ['lead.execution'] }),
  scene('listCommits', 'ghGetHistoryItem')({ ...R, operation: 'compare', baseRef: 'aaa', headRef: 'bbb', sections: ['files'] }, { channels: ['listCommits'] }, { github: cappedCompare, allow: ['lead.execution'] }),
  scene('readCommitFiles', 'ghGetHistoryItem')({ ...R, operation: 'compare', baseRef: 'aaa', headRef: 'bbb' }, { channels: ['readCommitFiles'] }, { github: cappedCompare, allow: ['lead.execution'] }),
  scene('commentBody', 'ghGetHistoryItem')({ ...R, operation: 'issue', number: 2, sections: ['comments'] }, { channels: ['nextPage'] }, { github: request => (request.p === '/repos/octo/demo/issues/2/comments' ? { body: [{ id: 21, user: { login: 'octo-dev', type: 'User' }, body: 'long comment text '.repeat(1000), created_at: '2025-01-01T00:00:00Z', updated_at: '2025-01-01T00:00:00Z', html_url: 'x', author_association: 'OWNER' }] } : null) }),
  scene('providerCommitListLimit', 'ghGetHistoryItem')({ ...PR, sections: ['commits'] }, { reasons: ['providerCommitListLimit'] }, { github: request => (request.p === '/repos/octo/demo/pulls/7' ? { body: { number: 7, title: 't', state: 'open', commits: 400, changed_files: 0, head: { sha: 'e'.repeat(40), ref: 'h' }, base: { sha: 'f'.repeat(40), ref: 'main' }, user: { login: 'u' }, created_at: '2025-01-01T00:00:00Z', updated_at: '2025-01-01T00:00:00Z' } } : request.p === '/repos/octo/demo/pulls/7/commits' ? { body: Array.from({ length: Math.min(100, 250) }, (_, index) => ({ sha: index.toString(16).padStart(40, '0'), commit: { message: `c${index}`, author: { name: 'n', date: '2025-01-01T00:00:00Z' } } })) } : null) }),
];

function cappedCompare(request) {
  const resolved = /^\/repos\/octo\/demo\/commits\/(aaa|bbb)$/.exec(request.p);
  if (resolved) return { body: { sha: resolved[1][0].repeat(40), commit: { message: resolved[1], author: { name: 'n', date: '2025-01-01T00:00:00Z' } }, parents: [] } };
  if (request.p.startsWith('/repos/octo/demo/compare/')) return { body: { status: 'ahead', ahead_by: 2, behind_by: 0, total_commits: 2, base_commit: { sha: 'a'.repeat(40) }, merge_base_commit: {}, commits: [1, 2].map(index => ({ sha: String(index).repeat(40), commit: { message: `c${index}`, author: { name: 'n', date: '2025-01-01T00:00:00Z' } }, parents: [] })), files: Array.from({ length: 300 }, (_, index) => file(`f${index}.ts`, { patch: '@@ -1 +1 @@\n+x' })) } };
  return null;
}

const READS = [
  scene('unfiltered.local', 'localFetch')({ path: '$DIR/a.txt', ...predicates('fetch-unknown') }, { channels: ['unfilteredUnknown'] }, { files: { 'a.txt': MANY_LINES }, provider: mixedRead }),
  scene('unfiltered.github', 'ghGetFileContent')({ ...R, path: 'long.txt', ref: 'a'.repeat(40), ...predicates('gh-fetch') }, { channels: ['unfilteredUnknown'] }, { github: contentOf('long.txt', MANY_LINES), provider: mixedRead }),
  scene('unfiltered.search', 'localSearch')({ path: '$DIR', matchString: ['evidence'], ...predicates('search-mixed') }, { channels: ['unfiltered', 'unfilteredUnknown'] }, {
    files: { 'keep.txt': 'evidence keep\n', 'drop.txt': 'evidence drop\n', 'odd.txt': 'evidence odd\n' },
    provider: request => ({ body: classificationAnswer(request, question => score(question, JSON.stringify(request).includes('drop.txt') ? 'excluded' : JSON.stringify(request).includes('odd.txt') ? 'unknown' : 'selected')) }),
  }),
  scene('unfiltered.code', 'ghSearchCode')({ ...R, keywords: ['greeter'], ...predicates('code-mixed') }, { channels: ['unfiltered'] }, { provider: answering(() => 'excluded') }),
  scene('readHits', 'ghSearchCode')({ ...R, keywords: ['greeter'], ref: 'a'.repeat(40) }, { channels: ['readHits'], reasons: ['hitLinesNotListed'] }, {
    github: request => (request.p.startsWith('/repos/octo/demo/contents/') ? { status: 500, body: { message: 'unreadable' } } : null), allow: ['lead.execution'],
  }),
];

function contentOf(name, text) {
  return request => (request.p === `/repos/octo/demo/contents/${name}` ? { body: { type: 'file', encoding: 'base64', path: name, size: Buffer.byteLength(text), sha: 'a'.repeat(40), content: Buffer.from(text).toString('base64') } } : null);
}

const treeless = request => (/\/git\/trees\//.test(request.p) ? { status: 404, body: { message: 'Not Found' } } : null);
const STRUCTURE = [
  scene('expandDepth', 'ghStructure')({ ...R, operation: 'tree', materialize: true, maxDepth: 1, ref: 'a'.repeat(40) }, { channels: ['expandDepth'], reasons: ['materializeDepth'] }, { allow: ['lead.execution', 'lead.repeat'], github: request => treeless(request) ?? (request.p === '/repos/octo/demo/contents' ? { body: [{ type: 'file', name: 'one.rs', path: 'one.rs', size: 1, sha: 'a'.repeat(40) }, { type: 'dir', name: 'router', path: 'router', size: 0, sha: 'b'.repeat(40) }] } : request.p === '/repos/octo/demo/contents/one.rs' ? { body: { type: 'file', encoding: 'base64', path: 'one.rs', size: 2, sha: 'a'.repeat(40), content: 'eAo=' } } : null) }),
  scene('continueMaterialize', 'ghStructure')({ ...R, operation: 'tree', materialize: true, maxDepth: 1, ref: 'b'.repeat(40) }, { channels: ['continueMaterialize'], reasons: ['materializeIncomplete'] }, { allow: ['lead.execution'], github: request => treeless(request) ?? (request.p === '/repos/octo/demo/contents' ? { body: [{ type: 'file', name: 'a.rs', path: 'a.rs', size: 1, sha: 'a'.repeat(40) }, { type: 'file', name: 'b.rs', path: 'b.rs', size: 1, sha: 'b'.repeat(40) }] } : request.p === '/repos/octo/demo/contents/a.rs' ? { body: { type: 'file', encoding: 'base64', path: 'a.rs', size: 2, sha: 'a'.repeat(40), content: 'eAo=' } } : request.p === '/repos/octo/demo/contents/b.rs' ? { status: 429, body: { message: 'slow' }, headers: { 'retry-after': '1' } } : null) }),
  scene('partialTreeFailures', 'ghStructure')({ ...R, operation: 'tree', maxDepth: 2 }, { reasons: ['partialTreeFailures'] }, { allow: ['lead.repeat'], github: request => treeless(request) ?? (request.p === '/repos/octo/demo/contents' ? { body: [{ type: 'dir', name: 'src', path: 'src', size: 0, sha: 'b'.repeat(40) }] } : request.p === '/repos/octo/demo/contents/src' ? { status: 500, body: { message: 'down' } } : null) }),
  scene('providerContentsLimit', 'ghStructure')({ ...R, operation: 'tree', maxDepth: 1 }, { reasons: ['providerContentsLimit'] }, { github: request => treeless(request) ?? (request.p === '/repos/octo/demo/contents' ? { body: Array.from({ length: 1000 }, (_, index) => ({ type: 'file', name: `f${index}`, path: `f${index}`, size: 1, sha: 'a'.repeat(40) })) } : null) }),
];

const searchAnswer = body => request => (request.p === '/search/repositories' ? { body } : null);
const MISSING_VERSION = { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '9.9.9', registryUrl: '$MOCK' };

const failureText = report => report.failures.map(failure => failure.detail).join('; ');
const sameIdentity = query => query.operation === 'resolve' && query.ecosystem === MISSING_VERSION.ecosystem && query.packageName === MISSING_VERSION.packageName;

/** The nearest-version read a missing version offers, or why it is wrong. */
function nearestVersionLead(data) {
  if (data?.next?.repair) return { problem: 'a different version must be an optional related read' };
  const lead = data?.related?.find(call => call.tool === 'artifactSearch');
  if (lead?.kind !== 'related' || !lead.input?.queries?.length) return { problem: 'nearest-version related read missing' };
  const changed = lead.input.queries.some(query => !sameIdentity(query) || !query.version || query.version === MISSING_VERSION.version);
  return changed ? { problem: 'nearest-version read changed package identity or repeated the missing version' } : { lead };
}

async function readNearestVersions({ walk }) {
  const first = await walk('artifactSearch', { queries: [MISSING_VERSION] });
  if (first.failures.length) return failureText(first);
  const { problem, lead } = nearestVersionLead(first.pages[0]?.json?.results?.[0]?.data);
  if (problem) return problem;
  const read = await walk(lead.tool, lead.input);
  if (read.failures.length) return failureText(read);
  const versions = new Set(read.pages.flatMap(page => page.json?.results ?? [])
    .flatMap(row => row.data?.artifacts ?? []).map(artifact => artifact.version));
  return lead.input.queries.every(query => versions.has(query.version)) ? null : 'related read did not resolve every offered version';
}

const PROVIDER_SHAPES = [
  scene('providerResultCap', 'ghSearchRepo')({ operation: 'search', keywords: ['alpha'], limit: 1 }, { reasons: ['providerResultCap'] }, { github: request => (request.p === '/search/repositories' ? { body: { total_count: 5000, incomplete_results: false, items: [] } } : null), allow: ['lead.execution'] }),
  scene('providerIncompleteResults', 'ghSearchRepo')({ operation: 'search', keywords: ['alpha'] }, { reasons: ['providerIncompleteResults'], channels: [] }, { github: searchAnswer({ total_count: 0, incomplete_results: true, items: [] }), allow: ['lead.repeat'] }),
  // Selecting another version changes the request: these are related reads.
  // The distinct required page repair is exercised by native page-state tests.
  scene('repair.search', 'artifactSearch')(MISSING_VERSION, { codes: ['versionNotFound'] }, { run: readNearestVersions }),
  { id: 'lead.restart', tool: 'localFetch', files: { 'a.txt': Array.from({ length: 4000 }, (_, index) => `line ${index}\n`).join('') }, channels: ['restart'], codes: ['staleSnapshot'], run: async ({ call, dir }) => {
    const first = await call('localFetch', { queries: [{ path: '$DIR/a.txt' }], responseChars: 500 });
    const next = first.json?.responsePagination?.next ?? first.json?.results?.[0]?.data?.next?.nextPage;
    if (!next) return 'no page to resume';
    (await import('node:fs')).writeFileSync(`${dir}/a.txt`, 'changed text\n');
    await call(next.tool, next.input);
    return null;
  } },
];

const CYCLE = Object.fromEntries(Array.from({ length: 52 }, (_, index) => [`ring/m${index}.ts`, `import { f${(index + 1) % 52} } from './m${(index + 1) % 52}';\nexport function f${index}() { return f${(index + 1) % 52}; }\n`]));
const GRAPH = [{ id: 'lead.cycle', tool: 'graphQuery', files: CYCLE, channels: ['cycle0'], run: async ({ call, walk }) => {
  const built = await call('graphBuild', { queries: [{ path: '$DIR' }] });
  if (built.isError) return `build failed: ${built.text?.slice(0, 200)}`;
  const report = await walk('graphQuery', { queries: [{ operation: 'cycles', path: '$DIR' }] });
  return report.failures.length ? JSON.stringify(report.failures).slice(0, 300) : null;
} }];

export const LEAD_SCENARIOS = [...HISTORY, ...READS, ...STRUCTURE, ...PROVIDER_SHAPES, ...GRAPH, ...LSP_SCENARIOS];
