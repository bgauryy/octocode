// Remote surfaces: GitHub discovery/read/history, package search, clasify.
// Each result must answer, and every next.* page and hints.* lead it offers (one level) must execute.
import { checks, clasifyUnavailable, collect, findHint, nextHints, rowData, startServer, writeResults } from './mcp-client.mjs';

const { check, skip, summary } = checks('remote');
const client = await startServer();
const noClasify = clasifyUnavailable(client);
const { call, raw, follow } = client;
const OWNER = 'microsoft';
const REPO = 'TypeScript';

async function followAll(entry, label) {
  const hints = nextHints(entry.sc).slice(0, 4);
  for (const h of hints) {
    if (h.tool === 'clasify' && noClasify) { skip(`${label}: ${h.path.replace(/^\.results\.\d+\.data\./, '')} executes`, noClasify); continue; }
    const out = await follow(h);
    if (h.query?.queries?.[0]?.ref && /\.(viewReleaseSource|readManifest)$/.test(h.path) && out.rowErrors) {
      // An unpushed release ref recovers on the default branch: the same lead without ref.
      const unavailable = collect(out.sc, o => o.errorCode === 'notFound').length > 0;
      // AR2: only the registry's unchecked claim (`registryRef`, stated on the row) may be unpushed; a `provenance`/`tag` ref must open.
      const row = entry.sc?.results?.[Number(/^\.results\.(\d+)\./.exec(h.path)?.[1] ?? 0)]?.data?.artifacts?.[0];
      check(`${label}: unavailable release ref is the row's registryRef`, unavailable && row?.verification === 'registryRef', JSON.stringify({ lead: h, verification: row?.verification }));
      const { ref, ...recovery } = h.query.queries[0];
      const recovered = await follow({ tool: h.tool, query: { queries: [recovery] } });
      check(`${label}: unavailable release lead recovers without ref`, !recovered.isError && !recovered.rowErrors && (h.tool !== 'ghStructure' || (rowData(recovered)?.entries ?? []).length > 0), recovered.text.slice(0, 100));
      continue;
    }
    check(`${label}: ${h.path.replace(/^\.results\.\d+\.data\./, '')} executes`, !/Input validation error/.test(out.text) && !out.isError && !out.rowErrors, out.text.slice(0, 100).replace(/\s+/g, ' '));
  }
  return hints.length;
}

const repos = await call('ghSearchRepo', { keywords: ['typescript', 'compiler'], owner: OWNER, pageSize: 5 });
check('ghSearchRepo answers', !repos.isError && !repos.rowErrors && /TypeScript/.test(repos.text), repos.text.slice(0, 100));
await followAll(repos, 'ghSearchRepo');

const code = await call('ghSearchCode', { owner: OWNER, repo: REPO, keywords: ['NewChecker'], pageSize: 5 });
check('ghSearchCode answers with paths', !code.isError && collect(rowData(code), o => typeof o.path === 'string').length > 0, code.text.slice(0, 120));
const readTop = findHint(code.sc, 'hints.read');
check('ghSearchCode offers hints.read', !!readTop);
if (readTop) {
  const top = await follow(readTop);
  check('hints.read reads the matching source', /NewChecker/i.test(top.text), top.text.slice(0, 120));
}

const tree = await call('ghStructure', { owner: OWNER, repo: REPO, maxDepth: 1, pageSize: 50 });
check('ghStructure lists paths', !tree.isError && !tree.rowErrors && /package\.json|README/.test(tree.text), tree.text.slice(0, 100));
await followAll(tree, 'ghStructure');

const file = await call('ghGetFileContent', { owner: OWNER, repo: REPO, path: 'README.md', matchString: 'TypeScript', contextLines: 1 });
check('ghGetFileContent match window', !file.isError && !file.rowErrors && /TypeScript/.test(rowData(file)?.content ?? file.text), file.text.slice(0, 100));
const outline = await call('ghGetFileContent', { owner: OWNER, repo: REPO, path: 'package.json', ranges: ['1-10'] });
check('ghGetFileContent bounded range', !outline.isError && /"name"/.test(outline.text), outline.text.slice(0, 100));

const prs = await call('ghSearchHistory', { operation: 'pullRequest', owner: OWNER, repo: REPO, state: 'merged', pageSize: 3 });
const prNumber = collect(rowData(prs), o => typeof o.number === 'number')[0]?.number;
check('ghSearchHistory pullRequest returns numbers', !!prNumber, prs.text.slice(0, 120));
if (prNumber) {
  const pr = await call('ghGetHistoryItem', { operation: 'pullRequest', owner: OWNER, repo: REPO, number: prNumber });
  check(`ghGetHistoryItem pullRequest #${prNumber}`, !pr.isError && !pr.rowErrors, pr.text.slice(0, 100));
  await followAll(pr, 'ghGetHistoryItem pullRequest');
}
const commits = await call('ghSearchHistory', { operation: 'commit', owner: OWNER, repo: REPO, pageSize: 2 });
const sha = collect(rowData(commits), o => typeof o.sha === 'string' || typeof o.oid === 'string')[0];
check('ghSearchHistory commit returns SHAs', !!sha, commits.text.slice(0, 120));
if (sha) {
  const commit = await call('ghGetHistoryItem', { operation: 'commit', owner: OWNER, repo: REPO, ref: sha.sha ?? sha.oid });
  check('ghGetHistoryItem commit', !commit.isError && !commit.rowErrors, commit.text.slice(0, 100));
}
const issues = await call('ghSearchHistory', { operation: 'issue', owner: OWNER, repo: REPO, state: 'closed', pageSize: 2 });
check('ghSearchHistory issue', !issues.isError && collect(rowData(issues), o => typeof o.number === 'number').length > 0, issues.text.slice(0, 100));

const exact = await call('artifactSearch', { ecosystem: 'npm', packageName: 'typescript' });
check('artifactSearch exact npm package', !exact.isError && /typescript/i.test(exact.text), exact.text.slice(0, 100));
await followAll(exact, 'artifactSearch exact');
const discover = await call('artifactSearch', { ecosystem: 'npm', keywords: ['yaml', 'parser'], pageSize: 5 });
check('artifactSearch keyword discovery', !discover.isError && !discover.rowErrors, discover.text.slice(0, 100));

if (noClasify) skip('clasify supplied-context judgment answers', noClasify);
else {
  const judged = await raw('clasify', {
    queries: [{
      mainGoal: 'Judge supplied code',
      reasoning: 'held-state judgment smoke',
      resources: [{ id: 'r1', value: 'export function add(a, b) { return a + b; }' }],
      questions: [{ id: 'q1', type: 'yesno', ask: 'Does this code define a function named add?' }],
    }],
  });
  check('clasify supplied-context judgment answers', !/Input validation error/.test(judged.text) && !judged.isError && !judged.rowErrors, judged.text.slice(0, 160).replace(/\s+/g, ' '));
}

const result = summary();
writeResults('remote', { ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
