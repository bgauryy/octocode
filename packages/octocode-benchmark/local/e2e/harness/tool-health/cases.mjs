// Public behavior fixtures. Inputs are authored envelopes; cursors are only
// followed from results. Tiny display budgets exercise transport pagination.
import fs from 'node:fs';
import path from 'node:path';
import { declarations, lspLocations, structureFiles, sourceView, inventoryRows } from '../responses.mjs';

const files = data => (data?.files ?? []).flatMap(file => (file.matches ?? []).map(m => `${file.path}:${m.line}:${m.column ?? 0}`));
const numbered = text => sourceView({ content: text ?? '' }).ranges.flatMap(r => Array.from({ length: r.end - r.start + 1 }, (_, i) => r.start + i));
const lineNumbers = text => Array.from({ length: text.split('\n').length - Number(text.endsWith('\n')) }, (_, i) => i + 1);
const fileRows = data => (data?.pullRequests ?? []).flatMap(pr => inventoryRows(pr.files).map(f => `${pr.number}:${f.path}:${f.status}`));
const treeRows = data => structureFiles(data.files ?? []).map(f => f.path);
const page = query => ({ input: { queries: [query], responseChars: 150 }, lead: 'responsePagination.next' });
const LOCAL = 6000, REMOTE = 6000;
const repo = { owner: 'octo', repo: 'demo' };
const source = { tool: 'localFetch', query: { path: 'src/greeter.ts', ranges: [{ startLine: 1, endLine: 4 }] } };
const questions = { named: { type: 'noul', instructions: 'Does this declare a field named text?' } };

function readerWorkspace(ctx) {
  const dir = path.join(ctx.tmp, 'reader-workspace');
  fs.cpSync(ctx.fixture, dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'large.txt'), Array.from({ length: 1500 }, (_, i) => `line ${i + 1}: 界😀 immutable source evidence\n`).join(''));
  return dir;
}

export const CASES = {
  localSearch: {
    budgetMs: LOCAL,
    happy: { query: { path: 'src', matchString: ['healthMarker'] }, fields: ['files', 'matchCount'], rows: files, min: 4 },
    page: page({ path: 'src', matchString: ['healthMarker'] }),
    empty: { query: { path: 'src', matchString: ['zzzNoSuchToken'] } },
    invalid: { query: { path: 'src', matchString: 'healthMarker' } },
    explore: [
      { label: 'literal', query: { path: 'src', matchString: ['greet(Loudly)?('] }, class: 'empty' },
      { label: 'regex', query: { path: 'src', matchString: ['greet(Loudly)?\\('], regexEngine: 'rust' } },
      { label: 'asymmetricContext', query: { path: 'src', matchString: ['greet'], contextLines: { before: 0, after: 3 } } },
    ],
  },
  structureSearch: {
    budgetMs: LOCAL,
    happy: { query: { path: '.', operation: 'files' }, fields: ['files', 'summary'], rows: treeRows, min: 5 },
    page: page({ path: '.', operation: 'files' }),
    empty: { query: { path: '.', operation: 'files', include: ['**/zzzNoSuchName*'] } },
    invalid: { query: { operation: 'files' } },
    explore: [{ label: 'tree', query: { path: '.', operation: 'tree', maxDepth: 3 } }],
  },
  astSearch: {
    budgetMs: LOCAL,
    happy: { query: { path: 'src', operation: 'symbols' }, fields: ['files'], rows: data => declarations({ sc: { results: [{ data }] } }).map(d => `${d.path}:${d.name}:${d.line}`), min: 6 },
    page: page({ path: 'src', operation: 'symbols' }),
    empty: { query: { path: 'src', operation: 'match', rule: { pattern: 'zzzNoSuchCall($A)' }, parser: 'typescript' } },
    invalid: { query: { path: 'src', operation: 'match' } },
    explore: [{ label: 'match', query: { path: 'src', operation: 'match', rule: { pattern: 'greet($A)' }, parser: 'typescript' } }],
  },
  localFetch: {
    budgetMs: LOCAL, workspace: readerWorkspace,
    happy: { query: { path: 'src/greeter.ts' }, fields: ['content', 'totalLines'], rows: data => numbered(data.content), min: 18 },
    page: { query: { path: 'large.txt' }, lead: 'continueWindow', expect: () => Array.from({ length: 1500 }, (_, i) => i + 1) },
    empty: { query: { path: 'src/greeter.ts', matchString: ['zzzNoSuchText'] } },
    invalid: { query: { path: 'src/greeter.ts', ranges: ['1-4'] } },
    explore: [
      { label: 'ranges', query: { path: 'src/app.ts', ranges: [{ startLine: 1, endLine: 2 }, { startLine: 6, endLine: 8 }] }, check: data => numbered(data.content).join() === '1,2,6,7,8' ? null : 'range coordinates changed' },
      { label: 'symbols', query: { path: 'src/greeter.ts', view: 'symbols' } },
    ],
  },
  lspSearch: {
    budgetMs: 25000, needs: 'lsp',
    happy: { query: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references' }, fields: ['matches', 'matchCount'], rows: data => lspLocations({ sc: { results: [{ data }] } }).map(l => `${l.path}:${l.displayRange.startLine}:${l.displayRange.startCharacter}`), min: 6 },
    page: page({ path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references' }),
    empty: { query: { operation: 'workspaceSymbol', workspaceRoot: '.', symbolQuery: 'zzzNoSuchSymbol' } },
    invalid: { query: { path: 'src/greeter.ts', symbolName: 'greet' } },
    explore: [{ label: 'callers', query: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'callers' } }],
  },
  ghSearchCode: {
    budgetMs: REMOTE,
    happy: { query: { ...repo, keywords: ['greet'] }, fields: ['files'], rows: data => (data.files ?? []).map(f => f.path), min: 3 },
    page: page({ ...repo, keywords: ['greet'] }),
    empty: { query: { ...repo, keywords: ['zzznosuchtoken'] } },
    invalid: { query: { ...repo, keywords: 'greet' } },
  },
  ghGetFileContent: {
    budgetMs: REMOTE,
    happy: { query: { ...repo, path: 'src/greeter.ts' }, fields: ['content', 'totalLines'], rows: data => numbered(data.content), min: 18 },
    page: { query: { owner: 'octo', repo: 'demo-docs', path: 'guide.txt' }, lead: 'continueWindow', expect: ctx => lineNumbers(ctx.mock.model.repos.find(r => r.name === 'demo-docs').files['guide.txt']) },
    empty: { query: { ...repo, path: 'src/greeter.ts', matchString: ['zzzNoSuchText'] } },
    invalid: { query: repo },
    explore: [{ label: 'ranges', query: { ...repo, path: 'src/app.ts', ranges: [{ startLine: 1, endLine: 3 }] } }],
  },
  ghStructure: {
    budgetMs: REMOTE,
    happy: { query: { ...repo, operation: 'tree', maxDepth: 3 }, fields: ['files', 'summary'], rows: treeRows, min: 7 },
    page: { query: { ...repo, operation: 'tree', ref: 'feature-4', include: ['**/mod1*.ts'] }, lead: 'expandDates', expect: ctx => ctx.mock.model.pulls.find(pr => pr.number === 4).head.added.filter(p => /^pkg\/mod1\d*\.ts$/.test(p)) },
    empty: { query: { ...repo, operation: 'tree', include: ['**/zzzNoSuchName*'] } },
    invalid: { query: { operation: 'tree', owner: 'octo' } },
    explore: [{ label: 'refs', query: { ...repo, operation: 'refs' }, check: data => data.tags?.includes('v1.0.0') && data.branches?.main ? null : 'fixture branch or tag missing' }],
  },
  ghSearchRepo: {
    budgetMs: REMOTE,
    happy: { query: { operation: 'search', keywords: ['demo'] }, fields: ['repositories'], rows: data => (data.repositories ?? []).map(r => `${r.owner}/${r.repo}`), min: 3 },
    page: page({ operation: 'search', keywords: ['demo'] }),
    empty: { query: { operation: 'search', keywords: ['zzznosuchrepo'] } },
    invalid: { query: { operation: 'search', keywords: ['demo'], sort: 'stars:desc' } },
    explore: [{ label: 'get', query: { ...repo, operation: 'get' } }],
  },
  ghSearchHistory: {
    budgetMs: REMOTE,
    happy: { query: { ...repo, operation: 'commit' }, fields: ['commits'], rows: data => (data.commits ?? []).map(c => c.commitSha), min: 3 },
    page: { query: { ...repo, operation: 'issue', keywords: ['Paged history'] }, lead: 'nextPage', rows: data => (data.issues ?? []).map(issue => issue.number), expect: ctx => ctx.mock.model.issues.filter(issue => issue.number >= 100).map(issue => issue.number) },
    empty: { query: { ...repo, operation: 'pullRequest', keywords: ['zzznosuch'] } },
    invalid: { query: { operation: 'commit', owner: 'octo' } },
    explore: [{ label: 'issues', query: { ...repo, operation: 'issue', keywords: ['prefix'] }, check: data => data.issues?.some(i => i.number === 2) ? null : 'fixture issue missing' }],
  },
  ghGetHistoryItem: {
    budgetMs: REMOTE,
    happy: { query: { ...repo, operation: 'pullRequest', number: 1, sections: ['files'] }, fields: ['pullRequests'], rows: fileRows, min: 2 },
    page: { query: { ...repo, operation: 'pullRequest', number: 4, sections: ['files'] }, lead: 'nextFilePage', expect: ctx => ctx.mock.model.pulls.find(pr => pr.number === 4).head.added.map(p => `4:${p}:added`) },
    empty: { query: { ...repo, operation: 'pullRequest', number: 1, sections: ['patches'], matchString: ['zzzNoSuchText'] } },
    invalid: { query: { ...repo, operation: 'pullRequest' } },
    explore: [{ label: 'issue', query: { ...repo, operation: 'issue', number: 2, sections: ['body', 'comments'] } }],
  },
  artifactSearch: {
    budgetMs: REMOTE,
    happy: { query: ctx => ({ operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', registryUrl: `${ctx.mock.url}/` }), fields: ['artifacts'], rows: data => (data.artifacts ?? []).map(a => `${a.packageName}@${a.version}`), min: 1, check: data => data.artifacts?.[0]?.dependencies?.includes('tiny-greet@^2.0.0') ? null : 'fixture dependency missing' },
    page: { input: ctx => ({ queries: [{ operation: 'search', ecosystem: 'npm', keywords: ['health'], registryUrl: `${ctx.mock.url}/` }], responseChars: 150 }), lead: 'responsePagination.next', unpaged: { query: ctx => ({ operation: 'search', ecosystem: 'npm', keywords: ['health'], registryUrl: `${ctx.mock.url}/` }) } },
    empty: { query: ctx => ({ operation: 'search', ecosystem: 'npm', keywords: ['zzznosuchpkg'], registryUrl: `${ctx.mock.url}/` }) },
    invalid: { query: { operation: 'resolve', ecosystem: 'npm' } },
  },
  ghCloneRepo: {
    budgetMs: 20000, needs: 'clone',
    happy: { query: repo, fields: ['location'], rows: data => [data.location?.commitSha].filter(Boolean), min: 1 },
    page: { na: 'a completed checkout has no result collection' },
    empty: { na: 'a checkout exists or fails' },
    invalid: { query: { owner: 'octo' } },
    extra: async ({ happy, mock }) => {
      const location = happy.cli.json?.results?.[0]?.data?.location;
      return [
        ['checkout.source', location?.localPath && fs.existsSync(path.join(location.localPath, 'src/greeter.ts')), 'clone contains fixture source'],
        ['checkout.revision', location?.commitSha === mock.model.head.oid, 'checkout matches requested remote head'],
        ['checkout.auth', mock.gitAuth.some(a => a.user === 'x-access-token' && a.tokenMatches), 'origin-scoped askpass used'],
      ];
    },
  },
  clasify: {
    mixedBatch: 'reject',
    budgetMs: REMOTE,
    happy: { query: { operation: 'evaluate', content: { fields: { text: 'string' } }, questions }, fields: ['answers', 'model', 'usage'], rows: data => Object.keys(data.answers ?? {}), min: 1, check: data => typeof data.answers?.named?.noul === 'number' ? null : 'typed answer missing' },
    page: { na: 'one small judgment is indivisible; large-answer walks are covered by native CLI/MCP integration' },
    empty: { na: 'a judgment answers or fails' },
    invalid: { query: { operation: 'evaluate', content: {}, questions: {} } },
    explore: [{ label: 'source', query: { operation: 'evaluate', source, questions }, check: data => data.next?.read?.kind === 'continue' ? null : 'captured inspection must resume saved evidence' }],
    extra: async ({ mock }) => {
      const sent = mock.classificationRequests.map(request => JSON.stringify(request));
      return [['source.selection', sent.some(text => text.includes('export interface Greeting')) && !sent.some(text => text.includes('healthMarker greeter-1')), 'provider receives only the selected source lines']];
    },
  },
};
