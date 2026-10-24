// Advanced remote behavior: assertions use explicit fixture facts, then drive
// the authored envelope and every required cursor through both built surfaces.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { inventoryRows, nextHints, sourceLines, structureFiles, rawPatch } from '../responses.mjs';
import { tokenFor } from './surfaces.mjs';
import { advancedRegistries } from './advanced-registry.mjs';

const REPO = { owner: 'octo', repo: 'demo' };
const data = result => result?.json?.results?.[0]?.data ?? {};
const equal = (actual, expected) => JSON.stringify(actual) === JSON.stringify(expected);
const names = rows => (rows ?? []).map(row => row.repo);
const paths = rows => (rows ?? []).map(row => row.path).toSorted();
const numbers = rows => (rows ?? []).map(row => row.number);
const sourceEvidence = result => !result?.isError && result?.json?.results?.[0]?.status === undefined
  && !data(result).errorCode && sourceLines(data(result)).length > 0;
const checks = async (ctx, cases) => {
  const out = [];
  for (const surface of ctx.surfaces) for (const [label, query, verify] of cases) {
    const result = await ctx.call(ctx.q(query), surface);
    out.push([`advanced.${label}.${surface}`, verify(data(result), result), JSON.stringify(result.json).slice(0, 700)]);
  }
  return out;
};
const error = errorCode => value => value.errorCode === errorCode;

async function repository(ctx) {
  const out = await checks(ctx, [
    ['search.rangeSort', { operation: 'search', keywords: ['demo'], filters: { stars: { min: 5, max: 50 }, language: 'TypeScript' }, sort: { by: 'stars', order: 'asc' } }, d => equal(names(d.repositories), ['demo-cli', 'demo'])],
    ['search.archiveOnly', { operation: 'search', owner: 'octo', filters: { archive: 'only' } }, d => equal(names(d.repositories), ['retired-greeter'])],
    ['list.defaultArchive', { operation: 'list', owner: 'octo' }, d => equal(names(d.repositories).toSorted(), ['demo', 'demo-cli', 'demo-docs'])],
    ['list.archiveOnly', { operation: 'list', owner: 'octo', filters: { archive: 'only' } }, d => equal(names(d.repositories), ['retired-greeter'])],
    ['get.fields', { ...REPO, operation: 'get', fields: ['stars'] }, d => d.repositories?.[0]?.stars === 42 && !Object.hasOwn(d.repositories[0], 'description') && d.repositories[0].repo === 'demo'],
    ['get.identity', { ...REPO, operation: 'get' }, d => d.repositories?.[0]?.owner === 'octo' && d.repositories?.[0]?.repo === 'demo'],
    ['get.missing', { ...REPO, operation: 'get', repo: 'missing-repository' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const token = tokenFor('ghSearchRepo', surface);
    ctx.mock.model.githubResponders.set(token, r => r.p === '/search/repositories' ? { body: { total_count: 0, incomplete_results: true, items: [] } } : null);
    try {
      const result = await ctx.call(ctx.q({ operation: 'search', keywords: ['partial-proof'] }), surface);
      const d = data(result);
      out.push([`advanced.search.providerPartial.${surface}`, d.isPartial === true && d.partialReasons?.includes('providerIncompleteResults'), JSON.stringify(d)]);
    } finally { ctx.mock.model.githubResponders.delete(token); }
  }
  return out;
}

async function code(ctx) {
  return checks(ctx, [
    ['filters.intersection', { ...REPO, keywords: ['greet'], pathPrefix: 'src', nameContains: 'greeter', extensions: ['ts', 'md'] }, d => equal(paths(d.files), ['src/greeter.ts'])],
    ['path.only', { ...REPO, keywords: ['greeter'], matchIn: ['path'] }, d => equal(paths(d.files), ['src/greeter.ts']) && d.files.every(file => !file.lines?.length)],
    ['ref.reRead', { ...REPO, keywords: ['healthMarker'], pathPrefix: 'src', ref: ctx.mock.model.head.oid }, d => d.commitSha === ctx.mock.model.head.oid && equal(paths(d.files), ['src/app.ts', 'src/greeter.ts', 'src/util.ts'])],
    ['scope.owner', { owner: 'octo', keywords: ['greeter'], extensions: ['md'] }, d => (d.files ?? []).some(file => file.path === 'README.md')],
  ]);
}

async function structure(ctx) {
  const out = await checks(ctx, [
    ['languages.bytes', { ...REPO, operation: 'languages' }, d => equal(d.languages, { TypeScript: 1200, Markdown: 300 })],
    ['refs.exact', { ...REPO, operation: 'refs', ref: ctx.mock.model.commits[0].oid }, d => d.commitSha === ctx.mock.model.commits[0].oid && d.refKind === 'commit'],
    ['tree.exactGlob', { ...REPO, operation: 'tree', include: ['src/greeter.ts'] }, d => equal(paths(structureFiles(d.files)), ['src/greeter.ts'])],
    ['tree.bareNameNoWidening', { ...REPO, operation: 'tree', include: ['greeter'] }, (d, r) => r.json?.results?.[0]?.status === 'empty' && !structureFiles(d.files).length],
    ['tree.branch', { ...REPO, operation: 'tree', ref: 'feature-4', include: ['pkg/mod00.ts', 'pkg/mod01.ts'] }, d => equal(paths(structureFiles(d.files)), ['pkg/mod00.ts', 'pkg/mod01.ts'])],
    ['tree.missingRef', { ...REPO, operation: 'tree', ref: 'missing-branch' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const result = await ctx.call(ctx.q({ ...REPO, operation: 'tree', include: ['src/greeter.ts'], materialize: true }), surface);
    const location = data(result).location;
    const target = location?.localPath && path.join(location.localPath, 'src/greeter.ts');
    out.push([`advanced.materialize.content.${surface}`, !!target && fs.existsSync(target) && fs.readFileSync(target, 'utf8').includes('return { text: `hello ${name}` };'), JSON.stringify(data(result))]);
  }
  return out;
}

async function reader(ctx) {
  const out = await checks(ctx, [
    ['ranges.exact', { ...REPO, path: 'src/greeter.ts', ranges: [{ startLine: 2, endLine: 3 }, { startLine: 8, endLine: 8 }] }, d => equal(sourceLines(d), [[2, 'export interface Greeting {'], [3, '  text: string;'], [8, '  return { text: `hello ${name}` };']])],
    ['matcher.orContext', { ...REPO, path: 'src/greeter.ts', matchString: ['greeter-1', 'greeter-2'], contextLines: { before: 0, after: 1 } }, d => equal(sourceLines(d).map(([line]) => line), [7, 8, 15, 16])],
    ['matcher.wholeWordCase', { ...REPO, path: 'src/greeter.ts', matchString: ['Greeter'], caseMode: 'sensitive', wholeWord: true }, d => equal(sourceLines(d).map(([line]) => line), [11])],
    ['matcher.pcre2', { ...REPO, path: 'src/greeter.ts', matchString: ['(?<=healthMarker )greeter-\\d'], regexEngine: 'pcre2' }, d => equal(sourceLines(d).map(([line]) => line), [7, 15])],
    ['matcher.regex', { ...REPO, path: 'src/greeter.ts', matchString: ['^export (function|class)'], regexEngine: 'rust' }, d => equal(sourceLines(d).map(([line]) => line), [6, 11])],
    ['view.compact', { ...REPO, path: 'src/greeter.ts', view: 'compact' }, d => d.view === 'compact' && d.content.includes('function greet') && !d.content.includes('healthMarker')],
    ['view.symbols', { ...REPO, path: 'src/greeter.ts', view: 'symbols' }, d => d.view === 'symbols' && d.content.includes('Greeting') && d.content.includes('say(') && !d.content.includes('return this.prefix')],
    ['ref.branch', { ...REPO, path: 'pkg/mod00.ts', ref: 'feature-4' }, d => equal(sourceLines(d), [[1, '// healthMarker bulk-0'], [2, 'export const mod0 = 0;']])],
    ['path.missing', { ...REPO, path: 'src/missing.ts' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghGetFileContent', { queries: [{ owner: 'octo', repo: 'demo-docs', path: 'guide.txt', forceRefresh: true }], responseChars: 4000 }, surface);
    const records = report.responses.filter(row => row.tool === 'ghGetFileContent').flatMap(row => row.result.json?.results?.flatMap(item => sourceLines(item.data)) ?? []);
    const expected = Array.from({ length: 1500 }, (_, i) => [i + 1, `Line ${i + 1}: the greeter guide repeats this sentence to fill a long document.`]);
    out.push([`advanced.cursor.exactEvidence.${surface}`, !report.failures.length && equal(records, expected), `${records.length} lines, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
    const first = report.responses[0]?.result;
    const lead = first && nextHints(first.json).find(call => call.kind === 'continue');
    if (lead) {
      const before = ctx.mock.requests.filter(request => request.includes('/contents/guide.txt')).length;
      const a = await ctx.callOn(surface)(lead.tool, lead.input);
      const b = await ctx.callOn(surface)(lead.tool, lead.input);
      const after = ctx.mock.requests.filter(request => request.includes('/contents/guide.txt')).length;
      const original = report.responses.find(row => row.tool === lead.tool && equal(row.input, lead.input))?.result;
      const exact = sourceEvidence(original) && sourceEvidence(a) && sourceEvidence(b)
        && equal(sourceLines(data(a)), sourceLines(data(original))) && equal(sourceLines(data(b)), sourceLines(data(original)));
      out.push([`advanced.cursor.immutableReplay.${surface}`, exact && before === after, `original page ${sourceLines(data(original)).length} lines; statuses ${[original,a,b].map(result => result?.json?.results?.[0]?.status).join(",")}; provider content calls ${before}→${after}`]);
    } else out.push([`advanced.cursor.issued.${surface}`, false, 'large source did not issue a continuation']);
  }
  return out;
}

async function history(ctx) {
  const out = await checks(ctx, [
    ['commits.pathDate', { ...REPO, operation: 'commit', pathPrefix: 'src', filters: { committedAt: { from: '2025-01-03T00:00:00Z', to: '2025-01-03T23:59:59Z' } } }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.head.oid])],
    ['commits.message', { ...REPO, operation: 'commit', keywords: ['src/greeter.ts'] }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.head.oid])],
    ['commits.ref', { ...REPO, operation: 'commit', ref: ctx.mock.model.commits[0].oid }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.commits[0].oid])],
    ['prs.open', { ...REPO, operation: 'pullRequest', filters: { state: 'open' } }, d => equal(numbers(d.pullRequests).toSorted((a, b) => a - b), [3, 4])],
    ['prs.merged', { ...REPO, operation: 'pullRequest', filters: { isMerged: true } }, d => equal(numbers(d.pullRequests), [1])],
    ['prs.branchSelection', { ...REPO, operation: 'pullRequest', filters: { headRef: { include: ['feature-4'], exclude: ['feature-3'] }, baseRef: { include: ['main'] } } }, d => equal(numbers(d.pullRequests), [4])],
    ['issues.author', { ...REPO, operation: 'issue', keywords: ['prefix'], filters: { author: { include: ['octo-dev'] } } }, d => equal(numbers(d.issues), [2])],
    ['issues.excludedAuthor', { ...REPO, operation: 'issue', keywords: ['prefix'], filters: { author: { exclude: ['octo-dev'] } } }, (d, r) => r.json?.results?.[0]?.status === 'empty' && !d.issues?.length],
  ]);
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghSearchHistory', { queries: [{ ...REPO, operation: 'issue', keywords: ['Paged history'] }], responseChars: 3000 }, surface);
    const actual = report.responses.filter(row => row.tool === 'ghSearchHistory').flatMap(row => row.result.json?.results?.flatMap(item => numbers(item.data?.issues)) ?? []);
    out.push([`advanced.issue.nativeAndResponsePaging.${surface}`, !report.failures.length && equal(actual, Array.from({ length: 51 }, (_, i) => 150 - i)), `${actual.length} issues, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
  }
  return out;
}

const files = d => inventoryRows(d.files ?? d.commits?.[0]?.files ?? d.pullRequests?.[0]?.files);

async function historyItem(ctx) {
  const out = await checks(ctx, [
    ['issue.discussion', { ...REPO, operation: 'issue', number: 2, sections: ['body', 'comments'] }, d => d.issues?.[0]?.body === 'The greeter needs a configurable prefix.' && d.issues?.[0]?.comments?.[0]?.body === 'Agreed, the Greeter class takes one now.'],
    ['commit.fileFilter', { ...REPO, operation: 'commit', ref: 'main', sections: ['files'], include: ['src/*.ts'], fileStatus: ['added'], minChanges: 10 }, d => equal(paths(files(d)), ['src/greeter.ts', 'src/util.ts'])],
    ['compare.files', { ...REPO, operation: 'compare', baseRef: ctx.mock.model.commits[0].oid, headRef: 'main', sections: ['files'], include: ['src/*.ts'] }, d => d.headSha === ctx.mock.model.head.oid && equal(paths(files(d)), ['src/app.ts', 'src/greeter.ts', 'src/util.ts'])],
    ['pr.sectionCommits', { ...REPO, operation: 'pullRequest', number: 4, sections: ['commits'] }, d => d.pullRequests?.[0]?.commits?.[0]?.commitSha === ctx.mock.model.branchCommits[0].oid],
    ['pr.patchMatch', { ...REPO, operation: 'pullRequest', number: 4, sections: ['patches'], include: ['pkg/huge.ts'], matchString: ['huge line 2999'], contextLines: { before: 1, after: 1 } }, d => { const patch = rawPatch(files(d)[0]?.patch); return patch.includes('+export const huge2998 = 2998; // healthMarker huge line 2999') && !patch.includes('huge line 1\n'); }],
    ['issue.missing', { ...REPO, operation: 'issue', number: 99999 }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'pullRequest', number: 4, sections: ['patches'], include: ['pkg/huge.ts'] }], responseChars: 6000 }, surface);
    const lines = report.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => files(item.data).flatMap(file => rawPatch(file.patch).split('\n').filter(line => line.startsWith('+export const')))) ?? []);
    const expected = Array.from({ length: 3000 }, (_, i) => `+export const huge${i} = ${i}; // healthMarker huge line ${i + 1}`);
    out.push([`advanced.patch.exactOnce.${surface}`, !report.failures.length && equal(lines, expected), `${lines.length} patch lines, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
    const commit = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'commit', ref: 'main', sections: ['patches'], include: ['src/greeter.ts'] }], responseChars: 20000 }, surface, { maxPages: 12 });
    const commitCalls = nextHints(commit.responses[0]?.result.json);
    const summary = commitCalls.find(call => call.kind === 'related' && call.tool === 'ghGetHistoryItem' && call.input.queries?.some(row => row.operation === 'commit' && !row.sections?.includes('patches')));
    out.push([`advanced.commit.requestedEvidenceTerminates.${surface}`, !commit.failures.length && commit.responses.length === 1 && !!summary, `${commit.responses.length} required calls; ${JSON.stringify(commit.failures)}`]);
    for (const section of ['reviews', 'reviewComments']) {
      const read = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'pullRequest', number: 4, sections: section === 'reviews' ? ['body', section] : [section] }], responseChars: 4000 }, surface);
      const evidence = read.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => (item.data?.pullRequests ?? []).flatMap(pr => (pr[section] ?? []).map(entry => [entry.id, entry.body]))) ?? []);
      const expectedReviews = Array.from({ length: 105 }, (_, i) => [String((section === 'reviews' ? 4000 : 5000) + i), `${section === 'reviews' ? 'Review' : 'Inline review'} evidence ${i}`]);
      out.push([`advanced.${section}.exactOnce.${surface}`, !read.failures.length && equal(evidence, expectedReviews), `${evidence.length} rows, ${read.responses.length} calls; ${JSON.stringify(read.failures)}`]);
    }
  }
  return out;
}

async function artifacts(ctx) {
  const registryUrl = ctx.mock.url;
  const out = await checks(ctx, [
    ['resolve.exact', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '1.0.0', registryUrl }, d => d.artifacts?.[0]?.version === '1.0.0' && equal(d.artifacts[0].dependencies, ['tiny-greet@^2.0.0'])],
    ['resolve.range', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '^1.0.0', registryUrl }, d => d.artifacts?.[0]?.version === '1.2.3'],
    ['resolve.tag', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-greet', version: 'latest', registryUrl }, d => d.artifacts?.[0]?.version === '2.1.0' && d.artifacts[0].sourceRef === ctx.mock.model.head.oid],
    ['resolve.missingVersion', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '9.0.0', registryUrl }, error('versionNotFound')],
    ['search.intersection', { operation: 'search', ecosystem: 'npm', keywords: ['health', 'greet'], registryUrl }, d => equal((d.artifacts ?? []).map(a => a.packageName), ['tiny-greet'])],
  ]);
  return [...out, ...await advancedRegistries(ctx)];
}

const count = cwd => Number(spawnSync('git', ['rev-list', '--count', 'HEAD'], { cwd, encoding: 'utf8' }).stdout?.trim());
const sparseFiles = (location, included, excluded) => !!location && fs.existsSync(path.join(location, included)) && !fs.existsSync(path.join(location, excluded));

async function clone(ctx) {
  const call = async query => data(await ctx.call(ctx.q({ ...REPO, ...query }), 'cli'));
  const first = await call({ paths: ['src'], historyDepth: 1 });
  const second = await call({ paths: ['docs'], historyDepth: 1 });
  const deep = await call({ paths: ['src'], historyDepth: 2 });
  const a = first.location?.localPath, b = second.location?.localPath, c = deep.location?.localPath;
  const out = [
    ['advanced.clone.sparseIdentity', a !== b && sparseFiles(a, 'src/greeter.ts', 'docs/notes.md') && sparseFiles(b, 'docs/notes.md', 'src/greeter.ts'), JSON.stringify({ first, second })],
    ['advanced.clone.depthIdentity', !!a && !!c && a !== c && count(a) === 1 && count(c) === 2, JSON.stringify(deep)],
  ];
  if (a) {
    const file = path.join(a, 'src/greeter.ts');
    fs.appendFileSync(file, '\n// local edit retained\n');
    const refreshed = await call({ paths: ['src'], historyDepth: 1, forceRefresh: true });
    out.push(['advanced.clone.preserveLocalEdits', refreshed.errorCode === 'checkoutDirty' && fs.readFileSync(file, 'utf8').endsWith('// local edit retained\n'), JSON.stringify(refreshed)]);
  }
  return out;
}

export const ADVANCED_REMOTE = {
  ghSearchRepo: repository, ghSearchCode: code, ghStructure: structure, ghGetFileContent: reader,
  ghSearchHistory: history, ghGetHistoryItem: historyItem, artifactSearch: artifacts, ghCloneRepo: clone,
};
