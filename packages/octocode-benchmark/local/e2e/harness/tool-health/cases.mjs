// Per-tool cases for harness/tool-health.mjs. Every contract tool needs one
// entry; the suite fails on a contract tool without a case or a case without a
// contract tool. Each case names, per check, a single query (wrapped as
// {queries:[query]}) or `{ na: reason }` when the check cannot apply to the
// tool; `rows(data, json)` turns a response row's data into stable row ids
// (used for continuation dedupe and paged-vs-unpaged completeness).
//
// Every seed (happy, page, empty, `explore`, `walks`) is explored by
// tool-health/driver.mjs: each lead it emits runs once and is judged.
//   page      the primary page channel `lead`, walked to its end; the union of
//             its rows equals the unpaged rows (`unpaged` query, else happy) or
//             `expect(ctx)`
//   walks     further page walks `{label, query, lead, rows?, unpaged?, expect?,
//             check?(rows), minPages?}`
//   channels  row ids per page channel key, for the driver's exactly-once check
//   explore   more seeds `{label, query, class?, check?(data, report)}`
//   workspace `(ctx) => dir`: the tool's cwd (CLI and its MCP server) instead of
//             fixtures/tool-health, for cases that write or need more files
//
// Local tools read fixtures/tool-health (the process cwd) or a temp copy of it.
// GitHub tools read the loopback mock's `octo/demo` (built from the same
// fixture files, plus the bulk pull request #4 of addBulkPullRequest);
// artifactSearch reads the mock npm registry; clasify the mock jev provider.
import { suggestedCall } from '../lead-predicates.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { declarations, lspLocations, structureFiles, sourceView, inventoryRows } from '../mcp-client.mjs';
import { dataOf, rowsOf } from './driver.mjs';
import { checkMaterialization } from '../github-materialize.mjs';

const files = data => (data?.files ?? []).flatMap(file => (file.matches ?? []).map(m => `${file.path}:${m.line}`));
const lines = text => Array.from({ length: text.split('\n').length - (text.endsWith('\n') ? 1 : 0) }, (_, i) => i + 1);
const numbered = text => sourceView({ content: text ?? '' }).ranges.flatMap(r => Array.from({ length: r.end - r.start + 1 }, (_, i) => r.start + i));

/** Local budgets cover a cold CLI process; lsp covers a cold language server. */
const LOCAL = 6000, REMOTE = 6000, LSP = 25000;

/** Patch rows of a ghGetHistoryItem response: `path:line` per numbered patch line (`N\t+text`). */
const patchRows = data => (data?.pullRequests ?? []).flatMap(pr => (pr.files ?? []).flatMap(f => (f.patch ?? '').split('\n').map(l => /^(\d+)\t/.exec(l)?.[1]).filter(Boolean).map(n => `${f.path}:${n}`)));
const fileRows = data => (data?.pullRequests ?? []).flatMap(pr => inventoryRows(pr.files).map(f => `#${pr.number}:${f.path}:${f.status}`));
/** ghStructure tree rows: every file and every listed folder (a page holds either). */
const treeRows = data => (data.files ?? []).flatMap(group => [
  ...structureFiles([{ ...group, files: group.files ?? [] }]).map(f => f.path),
  ...(group.folders ?? []).map(f => `${group.dir === '.' ? '' : `${group.dir}/`}${f}/`),
]);

/** ghCloneRepo: the explored leads into local tools rooted at the checkout, each answered with rows. */
function localFollowUps(report, localPath) {
  const local = (report?.calls ?? []).filter(c => c.label !== 'seed' && ['structureSearch', 'localSearch', 'localFetch', 'astSearch', 'lspSearch'].includes(c.tool));
  const outside = local.filter(c => !(c.query.queries ?? []).every(q => typeof q.path === 'string' && localPath && (q.path === localPath || q.path.startsWith(`${localPath}/`))));
  return [
    ['clone.followUps', local.length > 0, `local leads executed: ${local.map(c => `${c.label} (${c.cls})`).join(', ') || 'none'}`],
    ['clone.followUps.rooted', local.length > 0 && !outside.length, outside.length ? `lead outside the checkout: ${JSON.stringify(outside[0].query).slice(0, 160)}` : ''],
    ['clone.followUps.answered', local.length > 0 && local.every(c => c.cls === 'ok'), local.filter(c => c.cls !== 'ok').map(c => `${c.label}: ${c.cls}`).join('; ')],
  ];
}

// clasify: one noul, one choice and one score question over a state row and
// a source row (lines 1-4 of src/greeter.ts: the Greeting interface).
const GREETING_STATE = { declaration: 'Greeting', kind: 'interface', fields: { text: 'string' } };
const GREETING_SOURCE = { tool: 'localFetch', query: { path: 'src/greeter.ts', ranges: ['1-4'] } };
const CLASIFY_QUESTIONS = {
  named: { type: 'noul', instructions: 'Does the state declare a field named text?' },
  kind: { type: 'choice', instructions: 'Which declaration kind holds the text field?', criteria: { interface: 'an interface', class: 'a class', unclear: 'the state does not decide' } },
  typed: { type: 'score', instructions: 'How completely does the state type the text field?', criteria: ['untyped', 'loosely typed', 'exactly typed'] },
};
/** One row id per answer: `rowId:questionKey`. */
const answerRows = data => Object.keys(data?.answers ?? {}).map(key => `${data.id}:${key}`);
/** Why the answers do not match their questions' types (null when they all do). */
function answerShapes(data) {
  const answers = data?.answers ?? {};
  const keys = Object.keys(answers).sort().join(',');
  if (keys !== Object.keys(CLASIFY_QUESTIONS).sort().join(',')) return `answer keys ${keys}`;
  const p = x => typeof x === 'number' && x >= 0 && x <= 1;
  const { named, kind, typed } = answers;
  if (!p(named.noul)) return `noul ${JSON.stringify(named)}`;
  const labels = Object.keys(CLASIFY_QUESTIONS.kind.criteria);
  if (!labels.includes(kind.choice) || !p(kind.confidence) || Object.keys(kind.probabilities ?? {}).sort().join() !== [...labels].sort().join()) return `choice ${JSON.stringify(kind)}`;
  const levels = CLASIFY_QUESTIONS.typed.criteria.length;
  if (typeof typed.score !== 'number' || typed.score < 0 || typed.score > levels - 1 || !p(typed.confidence) || Object.keys(typed.legend ?? {}).length !== levels) return `score ${JSON.stringify(typed)}`;
  return null;
}
/** Why a source row's read lead does not re-read exactly the judged selection (null when it does). */
function readLeadProblem(data) {
  const read = data?.hints?.map(suggestedCall).find(call => call?.key === 'read');
  const row = read?.query?.queries?.[0];
  return read?.tool === 'localFetch' && String(row?.path).endsWith(GREETING_SOURCE.query.path) && JSON.stringify(row.ranges) === JSON.stringify(GREETING_SOURCE.query.ranges) && /^[a-f0-9]{64}$/.test(row.snapshot ?? '') && !Object.hasOwn(row, 'minify')
    ? null : `hints.read ${JSON.stringify(read)}`;
}

export const CASES = {
  localSearch: {
    budgetMs: LOCAL,
    happy: { query: { matchString: 'healthMarker', path: 'src' }, fields: ['files', 'matchCount'], rows: files, min: 4 },
    page: { query: { matchString: 'healthMarker', path: 'src', pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { matchString: 'zzzNoSuchToken', path: 'src' } },
    invalid: { query: { path: 'src' } },
    explore: [
      { label: 'files', query: { matchString: 'healthMarker', path: '.', resultView: 'files' } },
      { label: 'contextBefore', query: { matchString: 'healthMarker', path: 'src', contextBefore: 3, contextAfter: 0, pageSize: 1 } },
      { label: 'contextAfterClipped', query: { matchString: 'healthMarker', path: 'src', contextLines: 2, contextBefore: 0, contextAfter: 4, matchContentLength: 20 } },
      { label: 'ignoreCase', query: { matchString: 'HEALTHMARKER', path: 'src' }, class: 'empty' },
      { label: 'regex', query: { matchString: 'greet(Loudly)?\\(', regex: 'rust', path: 'src' }, deep: true },
      { label: 'countMatches', query: { matchString: 'greet', path: 'src', resultView: 'countMatches' }, deep: true },
    ],
  },
  structureSearch: {
    budgetMs: LOCAL,
    happy: { query: { path: '.', operation: 'files' }, fields: ['files', 'summary'], rows: data => structureFiles(data.files).map(f => f.path), min: 5 },
    page: { query: { path: '.', operation: 'files', pageSize: 2 }, lead: 'nextPage' },
    empty: { query: { path: '.', operation: 'files', include: ['zzzNoSuchName*'] } },
    invalid: { query: { operation: 'files' } },
    extra: async ({ callOn, surfaces }) => {
      const checks = [];
      for (const surface of surfaces) {
        const first = await callOn(surface)('structureSearch', { queries: [{ path: '.', operation: 'files', maxEntries: 1, pageSize: 1 }] });
        const lead = first.json?.results?.[0]?.data?.next?.continueWalk;
        const query = lead?.query?.queries?.[0];
        const valid = !first.isError && lead?.tool === 'structureSearch' && query?.maxEntries > 1 && query?.scanOffset === undefined;
        checks.push([`minimumScan.${surface}`, valid, valid ? '' : JSON.stringify(first.json)]);
        if (valid) {
          const resumed = await callOn(surface)(lead.tool, lead.query);
          checks.push([`minimumScan.resume.${surface}`, !resumed.isError, resumed.isError ? JSON.stringify(resumed.json) : '']);
        }
      }
      return checks;
    },
    explore: [
      { label: 'tree', query: { path: '.', operation: 'tree' } },
      { label: 'ts', query: { path: 'src', operation: 'files', extensions: ['ts'], pageSize: 1 }, deep: true },
    ],
  },
  astSearch: {
    budgetMs: LOCAL,
    happy: { query: { path: 'src', operation: 'symbols' }, fields: ['files'], rows: data => declarations({ sc: { results: [{ data }] } }).map(d => `${d.path}:${d.name}:${d.line}`), min: 6 },
    page: { query: { path: 'src', operation: 'symbols', pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { path: 'src', operation: 'match', pattern: 'zzzNoSuchCall($A)' } },
    invalid: { query: { path: 'src', operation: 'match' } },
    explore: [
      { label: 'match', query: { path: 'src', operation: 'match', pattern: 'greet($A)', language: 'typescript' } },
      { label: 'syntaxTree', query: { path: 'src/app.ts', operation: 'syntaxTree' }, deep: true },
      { label: 'byName', query: { path: 'src', operation: 'symbols', symbolName: 'greet' }, deep: true },
    ],
  },
  localFetch: {
    budgetMs: LOCAL,
    happy: { query: { path: 'src/greeter.ts', fullContent: true }, fields: ['path', 'content', 'totalLines'], rows: data => numbered(data.content), min: 18 },
    page: { query: { path: 'src/greeter.ts', unit: 'lines', offset: 0, length: 5 }, lead: 'continueWindow' },
    empty: { query: { path: 'src/greeter.ts', matchString: 'zzzNoSuchText' } },
    invalid: { query: { path: 'src/greeter.ts', ranges: 'not-an-array' } },
    // Raw-input parity (review R05): a call the canonical input schema rejects is
    // rejected by both surfaces as invalidInput, never repaired into a success.
    extra: async ({ callOn, surfaces, validateInput }) => {
      const out = [];
      for (const [label, row] of [
        ['stringBoolean', { path: 'src/greeter.ts', fullContent: 'true' }],
        ['enumCase', { path: 'src/greeter.ts', fullContent: true, minify: 'NONE' }],
        ['stringNumber', { path: 'src/greeter.ts', unit: 'lines', offset: '0', length: 5 }],
      ]) {
        const input = { queries: [{ mainGoal: 'Raw input parity.', ...row }] };
        const schemaError = validateInput('localFetch', input);
        out.push([`rawInput.${label}.schema`, !!schemaError, schemaError ? '' : `the canonical input schema accepts ${JSON.stringify(row)}`]);
        for (const surface of surfaces) {
          const res = await callOn(surface)('localFetch', input);
          const code = res.raw?.errorCode ?? res.json?.errorCode;
          out.push([`rawInput.${label}.${surface}`, res.isError && code === 'invalidInput', res.isError && code === 'invalidInput' ? '' : `${surface} accepted a schema-invalid call ${JSON.stringify(row)}: isError ${res.isError}, errorCode ${code}, status ${res.raw?.results?.[0]?.status ?? '-'}`]);
        }
      }
      return out;
    },
    explore: [
      { label: 'match', query: { path: 'src/greeter.ts', matchString: 'greet', contextLines: 1 } },
      { label: 'outline', query: { path: 'src/greeter.ts', minify: 'symbols' }, deep: true },
      { label: 'ranges', query: { path: 'src/app.ts', ranges: ['1-2', '6-8'] }, deep: true },
    ],
  },
  lspSearch: {
    budgetMs: LSP,
    needs: 'lsp',
    happy: { query: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references' }, fields: ['files', 'matchCount', 'coverage'], rows: data => lspLocations({ sc: { results: [{ data }] } }).map(l => `${l.path}:${l.displayRange.startLine}:${l.displayRange.startCharacter}`), min: 6 },
    page: { query: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'references', pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { operation: 'workspaceSymbol', workspaceRoot: '.', symbolName: 'zzzNoSuchSymbol' } },
    invalid: { query: { path: 'src/greeter.ts', symbolName: 'greet' } },
    explore: [
      { label: 'callers', query: { path: 'src/greeter.ts', symbolName: 'greet', line: 6, operation: 'callers' } },
      { label: 'definition', query: { path: 'src/app.ts', symbolName: 'greet', line: 6, operation: 'definition' }, deep: true },
      { label: 'documentSymbols', query: { path: 'src/greeter.ts', operation: 'documentSymbols' }, deep: true },
      { label: 'workspaceSymbol', query: { operation: 'workspaceSymbol', workspaceRoot: '.', symbolName: 'Greeter' }, deep: true },
    ],
  },
  ghSearchCode: {
    budgetMs: REMOTE,
    happy: { query: { owner: 'octo', repo: 'demo', keywords: ['greet'] }, fields: ['files', 'commitSha'], rows: data => (data.files ?? []).map(f => f.path), min: 3 },
    page: { query: { owner: 'octo', repo: 'demo', keywords: ['greet'], pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { owner: 'octo', repo: 'demo', keywords: ['zzznosuchtoken'] } },
    invalid: { query: { repo: 'demo', keywords: ['greet'] } },
    explore: [
      { label: 'path', query: { owner: 'octo', repo: 'demo', keywords: ['healthMarker'], path: 'src' } },
    ],
  },
  ghGetFileContent: {
    budgetMs: REMOTE,
    happy: { query: { owner: 'octo', repo: 'demo', path: 'src/greeter.ts', fullContent: true }, fields: ['path', 'content', 'commitSha', 'totalLines'], rows: data => numbered(data.content), min: 18 },
    // The mock's 1500-line octo/demo-docs guide.txt exceeds the full-content size limit.
    // Plain text keeps this case focused on exact continuation lines.
    page: { query: { owner: 'octo', repo: 'demo-docs', path: 'guide.txt', fullContent: true }, lead: 'continueWindow', expect: ctx => lines(ctx.mock.model.repos.find(r => r.name === 'demo-docs').files['guide.txt']) },
    empty: { query: { owner: 'octo', repo: 'demo', path: 'src/greeter.ts', matchString: 'zzzNoSuchText' } },
    invalid: { query: { owner: 'octo', repo: 'demo' } },
    explore: [
      { label: 'match', query: { owner: 'octo', repo: 'demo', path: 'src/greeter.ts', matchString: 'greet' } },
      { label: 'ranges', query: { owner: 'octo', repo: 'demo', path: 'src/app.ts', ranges: ['1-3'] }, deep: true },
    ],
  },
  ghStructure: {
    budgetMs: REMOTE,
    extra: checkMaterialization,
    happy: { query: { owner: 'octo', repo: 'demo', maxDepth: 3 }, fields: ['files', 'summary', 'commitSha'], rows: treeRows, min: 7 },
    page: { query: { owner: 'octo', repo: 'demo', maxDepth: 3, pageSize: 2 }, lead: 'nextPage', unpaged: { owner: 'octo', repo: 'demo', maxDepth: 3 } },
    empty: { query: { owner: 'octo', repo: 'demo', include: ['**/zzzNoSuchName*'] } },
    invalid: { query: { owner: 'octo' } },
    explore: [
      { label: 'refs', query: { owner: 'octo', repo: 'demo', operation: 'refs' } },
      { label: 'subtree', query: { owner: 'octo', repo: 'demo', path: 'src' }, deep: true },
    ],
  },
  ghSearchRepo: {
    budgetMs: REMOTE,
    happy: { query: { keywords: ['demo'] }, fields: ['repositories'], rows: data => (data.repositories ?? []).map(r => `${r.owner}/${r.repo}`), min: 3 },
    page: { query: { keywords: ['demo'], pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { keywords: ['zzznosuchrepo'] } },
    invalid: { query: { keywords: ['demo'], pageSize: 1000 } },
    explore: [
      { label: 'owner', query: { keywords: ['greeter'], owner: 'octo' }, deep: true },
    ],
  },
  ghSearchHistory: {
    budgetMs: REMOTE,
    happy: { query: { operation: 'commit', owner: 'octo', repo: 'demo' }, fields: ['commits'], rows: data => (data.commits ?? []).map(c => c.commitSha), min: 3 },
    explore: [
      { label: 'pullRequests', query: { operation: 'pullRequest', owner: 'octo', repo: 'demo' }, check: data => (data?.pullRequests ?? []).length >= 3 ? null : `pullRequests ${JSON.stringify(data?.pullRequests?.map(p => p.number))}` },
      { label: 'issues', query: { operation: 'issue', owner: 'octo', repo: 'demo' }, check: data => (data?.issues ?? []).some(i => i.number === 2) ? null : `issues ${JSON.stringify(data?.issues?.map(i => i.number))} lack #2` },
    ],
    page: { query: { operation: 'commit', owner: 'octo', repo: 'demo', pageSize: 1 }, lead: 'nextPage' },
    empty: { query: { operation: 'pullRequest', owner: 'octo', repo: 'demo', keywords: ['zzznosuch'] } },
    invalid: { query: { operation: 'commit', owner: 'octo' } },
  },
  ghGetHistoryItem: {
    budgetMs: REMOTE,
    happy: { query: { operation: 'pullRequest', owner: 'octo', repo: 'demo', number: 1, sections: ['files'] }, fields: ['pullRequests'], rows: data => (data.pullRequests ?? []).flatMap(pr => inventoryRows(pr.files).map(f => `#${pr.number}:${f.path}:${f.status}`)), min: 2 },
    // Pull request #4 (mock addBulkPullRequest): 36 changed files, one a 3000-line patch.
    page: { query: { operation: 'pullRequest', owner: 'octo', repo: 'demo', number: 4, sections: ['files'], pageSize: 10 }, lead: 'nextFilePage', rows: fileRows, unpaged: { operation: 'pullRequest', owner: 'octo', repo: 'demo', number: 4, sections: ['files'], pageSize: 100 }, minPages: 4 },
    walks: [
      { label: 'patch', query: { operation: 'pullRequest', owner: 'octo', repo: 'demo', number: 4, sections: ['patches'], include: ['pkg/huge.ts'] }, lead: 'continuePatch', rows: patchRows, expect: () => Array.from({ length: 3000 }, (_, i) => `pkg/huge.ts:${i + 1}`) },
    ],
    channels: { nextFilePage: fileRows, continuePatch: patchRows },
    empty: { query: { operation: 'pullRequest', owner: 'octo', repo: 'demo', number: 1, sections: ['patches'], matchString: 'zzzNoSuchText' } },
    invalid: { query: { operation: 'pullRequest', owner: 'octo', repo: 'demo' } },
    explore: [
      { label: 'issue', query: { operation: 'issue', owner: 'octo', repo: 'demo', number: 2, sections: ['body', 'comments'] } },
      { label: 'commit', query: { operation: 'commit', owner: 'octo', repo: 'demo', ref: 'main', sections: ['files'] }, deep: true },
      { label: 'compare', query: ctx => ({ operation: 'compare', owner: 'octo', repo: 'demo', base: ctx.mock.model.commits[0].oid, head: 'main', sections: ['files'] }), deep: true },
    ],
  },
  artifactSearch: {
    budgetMs: REMOTE,
    happy: { query: ctx => ({ ecosystem: 'npm', packageName: 'tiny-health', registryUrl: `${ctx.mock.url}/` }), fields: ['artifacts'], rows: data => (data.artifacts ?? []).map(a => `${a.name}@${a.version}`), min: 1, check: data => data.artifacts?.[0]?.dependencies?.includes('tiny-greet@^2.0.0') ? null : `dependencies ${JSON.stringify(data.artifacts?.[0]?.dependencies)} lack tiny-greet@^2.0.0` },
    page: { query: ctx => ({ ecosystem: 'npm', keywords: ['health'], pageSize: 1, registryUrl: `${ctx.mock.url}/` }), lead: 'nextPage', unpaged: ctx => ({ ecosystem: 'npm', keywords: ['health'], registryUrl: `${ctx.mock.url}/` }), min: 3 },
    empty: { query: ctx => ({ ecosystem: 'npm', keywords: ['zzznosuchpkg'], registryUrl: `${ctx.mock.url}/` }) },
    invalid: { query: { ecosystem: 'npm' } },
    explore: [
      { label: 'dependency', query: ctx => ({ ecosystem: 'npm', packageName: 'tiny-greet', registryUrl: `${ctx.mock.url}/` }), deep: true },
    ],
  },
  ghCloneRepo: {
    budgetMs: 20000,
    needs: 'clone',
    happy: { query: { owner: 'octo', repo: 'demo' }, fields: ['location'], rows: data => [data.location?.commitSha].filter(Boolean), min: 1 },
    // Until the runtime accepts a plain-http loopback endpoint for clones (as
    // it already does for the API base), the happy path cannot be hermetic.
    // Only that exact pre-change refusal skips; any other error fails.
    skipOnError: json => /requires an HTTPS GitHub API endpoint: /.test(JSON.stringify(json)) ? 'runtime refuses a plain-http loopback clone endpoint (approved runtime change pending: gh_clone_repo repository_url loopback http)' : null,
    extra: async ({ happy, happyRuns, mock, skips }) => {
      if (skips.length) return [];
      const location = happy.cli.json?.results?.[0]?.data?.location;
      const checkout = location?.localPath && fs.existsSync(path.join(location.localPath, 'src/greeter.ts'));
      return [
        ['clone.checkout', checkout, `localPath ${location?.localPath}`],
        ['clone.head', location?.commitSha === mock.model.head.oid, `commitSha ${location?.commitSha}, mock head ${mock.model.head.oid}`],
        // The token reaches git only through the GIT_ASKPASS helper for this origin.
        ['clone.askpass', mock.gitAuth.some(a => a.user === 'x-access-token' && a.tokenMatches), `git auth attempts ${JSON.stringify(mock.gitAuth)}`],
        // The clone's follow-up leads read the checkout locally: each ran in the
        // explore (accepted, contract-valid) and lists the checkout.
        ...localFollowUps(happyRuns.cli, location?.localPath),
      ];
    },
    page: { na: 'a checkout has no pages' },
    empty: { na: 'a checkout either exists or fails; there is no empty outcome' },
    invalid: { query: { owner: 'octo' } },
  },
  clasify: {
    budgetMs: REMOTE,
    // A state row judges held data; its answers are the whole outcome.
    happy: {
      query: { id: 'state', state: GREETING_STATE, questions: CLASIFY_QUESTIONS },
      fields: ['answers', 'model', 'usage'], rows: answerRows, min: 3,
      check: data => answerShapes(data) ?? (data.hints || data.source ? `a state row carries no source or read lead: ${JSON.stringify({ hints: data.hints, source: data.source })}` : null),
    },
    page: { na: 'a judged row has no page channel' },
    empty: { na: 'a judged row answers every question (no empty outcome)' },
    explore: [
      // A source row: native captures exactly the selection and leads back to it.
      { label: 'source', query: { id: 'source', source: GREETING_SOURCE, questions: CLASIFY_QUESTIONS }, check: data => answerShapes(data) ?? readLeadProblem(data) },
    ],
    invalid: { query: { state: GREETING_STATE, questions: {} } },
    extra: async ({ callOn, surfaces, cliCall, q, mock, validateOutput }) => {
      // The provider saw the selected lines and nothing past them.
      const sent = mock.classificationRequests.map(request => JSON.stringify(request));
      const selected = sent.some(text => text.includes('Tool-health fixture') && text.includes('export interface Greeting'));
      const beyond = sent.some(text => text.includes('healthMarker greeter-1'));
      const boundaryChecks = [];
      for (const surface of surfaces) {
        const criteria = Array.from({ length: 10 }, (_, i) => `level ${i}`);
        const question = { type: 'score', instructions: 'Rate the supplied state.', criteria };
        const before = mock.classificationRequests.length;
        const accepted = await callOn(surface)('clasify', q({ state: GREETING_STATE, questions: { boundary: question } }));
        const requests = mock.classificationRequests.slice(before);
        const forwarded = requests.flatMap(request => Object.values(request.questions ?? {}));
        const answer = dataOf(rowsOf(accepted.json)[0])?.answers?.boundary;
        boundaryChecks.push([`score.ten.${surface}`, !accepted.isError && !validateOutput('clasify', accepted.raw) && answer?.score === 9 && requests.length === 1 && forwarded.length === 1 && JSON.stringify(forwarded[0].criteria) === JSON.stringify(criteria), `${requests.length} provider POSTs; answer ${JSON.stringify(answer)}`]);
        const afterAccepted = mock.classificationRequests.length;
        const rejected = await callOn(surface)('clasify', q({ state: GREETING_STATE, questions: { boundary: { ...question, criteria: [...criteria, 'level 10'] } } }));
        boundaryChecks.push([`score.eleven.${surface}`, rejected.isError && rejected.json?.errorCode === 'invalidInput' && mock.classificationRequests.length === afterAccepted, `${mock.classificationRequests.length - afterAccepted} provider POSTs; errorCode ${rejected.json?.errorCode}`]);
      }
      // Disabled path: without a provider key the CLI refuses with a typed error.
      const res = await cliCall('clasify', q({ state: GREETING_STATE, questions: { named: CLASIFY_QUESTIONS.named } }), { env: { ['OCTOCODE_CLASSIFICATION_' + 'API']: '' } });
      const code = res.json?.errorCode ?? res.json?.queries?.[0]?.error?.errorCode;
      return [
        ...boundaryChecks,
        ['source.selection', selected && !beyond, `${sent.length} provider requests; selection sent ${selected}, lines past the range sent ${beyond}`],
        ['disabled.typed', res.status !== 0 && typeof code === 'string', `exit ${res.status}, errorCode ${code} ${res.stdout.slice(0, 120)}`],
      ];
    },
  },
};
