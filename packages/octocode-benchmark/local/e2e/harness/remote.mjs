// Opt-in acceptance against real providers and this checkout, through built CLI/MCP.
import fs from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { LIVE, checks, spawnCli, startServer, writeResults } from './mcp-client.mjs';
import { contract, contractValidators } from './contract.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { ROOT } from './paths.mjs';
import { sourceLines, declarations, structureFiles } from './responses.mjs';
import { createDriver, rowsOf, errorCodes } from './tool-health/driver.mjs';
import { transportProblem } from './tool-health/compare.mjs';

if (!LIVE) throw new Error('Live provider checks require OCTOCODE_LIVE=1');
const { check, summary } = checks('remote');
const env = { ...process.env, OCTOCODE_BETA: 'true', OCTOCODE_LSP_AUTO_INSTALL: 'off', PATH: `${path.join(ROOT, 'node_modules/.bin')}${path.delimiter}${process.env.PATH}` };
const provenance = { contract: contract.fingerprint, native: nativeBinaryStamp() };
const { validateInput, validateOutput } = contractValidators();
const presentation = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/cli-presentation.json'), 'utf8'));
const catalog = new Map(contract.tools.map(tool => [tool.name, tool]));
const driver = createDriver({ validate: validateOutput, validateInput, tools: new Set(catalog.keys()), cliOnly: new Set(contract.tools.filter(t => t.cliOnly).map(t => t.name)), schemaOf: tool => catalog.get(tool).outputSchema });
const client = await startServer({ env, timeoutMs: 90_000 });
const calls = [], exercised = new Set();
const data = result => rowsOf(result.json)[0]?.data;
const repo = { owner: 'octocat', repo: 'Hello-World' };
const localPath = 'packages/octocode-config/src/home.ts';
const localText = fs.readFileSync(path.join(ROOT, localPath), 'utf8');
const localLines = localText.split('\n');
if (localLines.at(-1) === '') localLines.pop();
const symbol = 'getOctocodeHome';
const line = localLines.findIndex(text => text.includes(`function ${symbol}(`)) + 1;
let publicSource;

async function call(surface, tool, input) {
  const invalid = validateInput(tool, input);
  let result;
  if (surface === 'mcp') {
    const entry = await client.raw(tool, input, '', { keepRaw: true });
    result = { ...entry, json: entry.raw };
  } else {
    const entry = await spawnCli([tool, JSON.stringify(input), '--json'], { env, timeoutMs: 90_000 });
    let json;
    try { json = JSON.parse(entry.stdout); } catch { /* The typed-response check reports malformed transport output. */ }
    result = { ...entry, json, raw: json, text: entry.stdout + entry.stderr, isError: entry.status > 1 };
  }
  const codes = errorCodes(result.json);
  const outputError = validateOutput(tool, result.json);
  check(`${tool}.${surface}.contract`, !outputError && (!invalid || codes.includes('invalidInput')), outputError ?? invalid ?? '');
  const transport = transportProblem(surface, result);
  check(`${tool}.${surface}.transport`, !transport, transport ?? '');
  calls.push({ tool, surface, input, ms: result.ms, errorCodes: codes, output: result.json });
  return result;
}

async function exercise(tool, query, inspect, { label = tool, responseChars = 4000 } = {}) {
  const results = {};
  for (const surface of catalog.get(tool).cliOnly ? ['cli'] : ['cli', 'mcp']) {
    const report = await driver.explore(tool, { queries: [query], responseChars }, {
      surface, maxCalls: 1, maxPages: 24, call: (name, input) => call(surface, name, input),
    });
    check(`${label}.${surface}.flow`, !report.failures.length && !report.root.isError && !errorCodes(report.root.json).length, report.failures.map(f => f.detail).join('; ') || errorCodes(report.root.json).join(','));
    check(`${label}.${surface}.evidence`, inspect(data(report.root), report.root), JSON.stringify(data(report.root) ?? report.root.json).slice(0, 160));
    results[surface] = report.root;
    exercised.add(`${tool}.${surface}`);
  }
  return results;
}

const sourceJudgment = url => ({ operation: 'evaluate', source: { url }, questions: { greeting: { type: 'noul', instructions: 'Does this text contain a greeting?' } } });

async function checkCapturedRead(surface, evidence, lines) {
  const lead = evidence?.next?.read;
  const executable = lead?.kind === 'continue' && lead.tool === 'localFetch';
  check(`clasify.publicRedirect.${surface}.capturedRead`, executable);
  if (!executable) return;
  const first = calls.findLast(entry => entry.surface === surface && entry.tool === lead.tool && isDeepStrictEqual(entry.input, lead.input));
  check(`clasify.publicRedirect.${surface}.exactCapture`, !!first && isDeepStrictEqual(sourceLines(rowsOf(first.output)[0]?.data), lines));
  const replay = await call(surface, lead.tool, lead.input);
  check(`clasify.publicRedirect.${surface}.savedReplay`, !replay.isError && !errorCodes(replay.json).length
    && isDeepStrictEqual(sourceLines(data(replay)), lines) && !data(replay)?.pagination?.hasMore);
}

async function publicSourceChecks({ ref, file, lines }) {
  const encodedFile = file.split('/').map(encodeURIComponent).join('/');
  const coordinate = `${repo.owner}/${repo.repo}/${ref}/${encodedFile}`;
  const rawUrl = `https://raw.githubusercontent.com/${coordinate}`;
  const inspect = d => typeof d?.answers?.greeting?.noul === 'number' && !!d?.usage && d?.source?.url === rawUrl;
  const direct = await exercise('clasify', sourceJudgment(rawUrl), inspect, { label: 'clasify.publicSource' });
  const redirected = await exercise('clasify', sourceJudgment(`https://github.com/${repo.owner}/${repo.repo}/raw/${ref}/${encodedFile}`), inspect, { label: 'clasify.publicRedirect' });
  for (const surface of ['cli', 'mcp']) {
    const evidence = data(redirected[surface]);
    check(`clasify.publicRedirect.${surface}.sameCapture`, typeof evidence?.source?.snapshot === 'string'
      && evidence.source.snapshot === data(direct[surface])?.source?.snapshot);
    await checkCapturedRead(surface, evidence, lines);
  }
}

try {
  check('fresh MCP exposes every eligible tool', contract.tools.filter(t => !t.cliOnly && !t.disabled).every(t => client.tools.some(listed => listed.name === t.name)));
  for (const listed of client.tools) {
    const expected = presentation.tools.find(tool => tool.name === listed.name);
    // JSON key order is irrelevant; compilation validates the actual advertised schema.
    check(`${listed.name}.discovery`, !!expected && [expected.mcp, expected.selection?.mcp].filter(Boolean).some(variant => isDeepStrictEqual(listed.inputSchema, variant.inputSchema)));
  }
  await exercise('ghSearchRepo', { operation: 'get', ...repo }, d => d?.repositories?.some(r => r.owner === repo.owner && r.repo === repo.repo));
  const history = await exercise('ghSearchHistory', { operation: 'commit', ...repo }, d => d?.commits?.some(c => /^[a-f0-9]{40}$/i.test(c.commitSha)));
  const ref = data(history.cli)?.commits?.[0]?.commitSha;
  if (ref) {
    await exercise('ghGetHistoryItem', { operation: 'commit', ...repo, ref }, d => d?.commitSha === ref);
    const tree = await exercise('ghStructure', { operation: 'tree', ...repo, ref }, d => structureFiles(d?.files ?? []).length > 0);
    const file = structureFiles(data(tree.cli)?.files ?? []).find(entry => /readme/i.test(entry.path))?.path;
    check('repository discovery produces a readable path', !!file);
    if (file) {
      const remote = await exercise('ghGetFileContent', { ...repo, path: file, ref }, d => sourceLines(d).length > 0);
      publicSource = { ref, file, lines: sourceLines(data(remote.cli)) };
      const text = sourceLines(data(remote.cli)).map(([, content]) => content).join('\n');
      await exercise('ghSearchCode', { ...repo, keywords: ['Hello'], ref }, d => d?.files?.some(hit => hit.path === file));
      await exercise('ghCloneRepo', { ...repo, ref, historyDepth: 1 }, d => d?.location?.commitSha === ref && fs.readFileSync(path.join(d.location.localPath, file), 'utf8').trimEnd() === text.trimEnd());
    }
  }
  await exercise('artifactSearch', { operation: 'resolve', ecosystem: 'npm', packageName: 'is-number', version: '7.0.0' }, d => d?.artifacts?.some(a => a.packageName === 'is-number' && a.version === '7.0.0'));
  await exercise('localSearch', { path: localPath, matchString: [symbol] }, d => d?.files?.some(file => file.matches?.some(match => match.line === line)));
  await exercise('structureSearch', { path: 'packages/octocode-config/src', operation: 'files', nameRegex: '^home\\.ts$' }, d => structureFiles(d?.files ?? []).some(file => file.path.endsWith('home.ts')));
  await exercise('localFetch', { path: localPath }, d => sourceLines(d).map(([, text]) => text).join('\n') === localLines.join('\n'));
  await exercise('astSearch', { path: localPath, operation: 'symbols' }, (d, r) => declarations({ sc: r.json }).some(declaration => declaration.name === symbol && declaration.line === line));
  await exercise('lspSearch', { path: localPath, operation: 'hover', symbolName: symbol, line }, d => JSON.stringify(d).includes(symbol));
  const built = await exercise('graphBuild', { path: 'packages/octocode-config/src' }, d => typeof d?.id === 'string' && d?.scan?.filesScanned > 0);
  const graph = data(built.cli)?.id;
  if (graph) await exercise('graphQuery', { operation: 'find', path: data(built.cli).root, graph, target: symbol }, d => d?.results?.some(row => row.name === symbol));
  await exercise('clasify', { operation: 'models' }, d => d?.models?.length > 0 && d.models.some(model => model.default), { label: 'clasify.models' });
  await exercise('clasify', { operation: 'evaluate', content: 'The color is blue.', questions: { blue: { type: 'noul', instructions: 'Does the text explicitly say the color is blue?' } } }, d => typeof d?.answers?.blue?.noul === 'number' && !!d?.usage, { label: 'clasify.evaluate' });
  if (publicSource) await publicSourceChecks(publicSource);
  await exercise('clasify', { operation: 'evaluate', content: { image: 'avatar' }, assets: { avatar: { source: { url: 'https://avatars.githubusercontent.com/u/583231?v=4' } } }, questions: { visible: { type: 'noul', instructions: 'Is a character visible in avatar?' } } }, d => typeof d?.answers?.visible?.noul === 'number' && d?.assets?.avatar?.width >= 280 && fs.statSync(d.assets.avatar.path).size > 0, { label: 'clasify.publicImage' });
  for (const tool of contract.tools.filter(t => !t.disabled)) {
    for (const surface of tool.cliOnly ? ['cli'] : ['cli', 'mcp']) {
      const rejected = await call(surface, tool.name, { queries: [], unexpectedField: true });
      check(`${tool.name}.${surface}.rejectsMalformed`, errorCodes(rejected.json).includes('invalidInput') && (surface === 'cli' ? rejected.status === 2 : rejected.isError));
    }
  }
} catch (error) {
  check('live execution completes', false, error.stack);
} finally { client.close(); }
for (const tool of contract.tools.filter(t => !t.disabled)) {
  for (const surface of tool.cliOnly ? ['cli'] : ['cli', 'mcp']) check(`${tool.name}.${surface}.covered`, exercised.has(`${tool.name}.${surface}`));
}
provenance.stable = !!provenance.native && provenance.native.sha256 === nativeBinaryStamp()?.sha256;
check('built native artifact unchanged during acceptance', provenance.stable);
const result = summary();
writeResults('remote', { ...result, provenance, calls, coverage: [...exercised] });
process.exitCode = result.failed.length ? 1 : 0;
