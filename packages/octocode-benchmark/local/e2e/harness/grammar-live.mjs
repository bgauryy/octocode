// Opt-in semantic checks against installed servers. Missing servers fail the requested coverage.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ROOT } from './paths.mjs';
import { spawnCli, startServer, writeResults } from './mcp-client.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { contract, contractValidators } from './contract.mjs';
import { createDriver } from './tool-health/driver.mjs';
import { transportPagesProblem } from './tool-health/compare.mjs';
import { stopTestBrokers } from './tool-health/mock-lsp-config.mjs';
import { lspLocations } from './responses.mjs';
import { GRAMMAR_FIXTURES } from './grammar-fixtures.mjs';
import { grammarAnchors } from './grammar-lsp.mjs';
import { configureRealServers, createLiveProject, liveServerEnvironment } from './grammar-live-projects.mjs';
import { semanticLocationEvidence, selectedGrammarCases } from './grammar-live-oracle.mjs';
import { captureProviderVersions, captureServerLogs } from './grammar-live-evidence.mjs';
import { semanticPagesComplete, settleSemanticQuery } from './grammar-live-readiness.mjs';

if (process.env.OCTOCODE_LIVE !== '1') throw new Error('grammar-live requires OCTOCODE_LIVE=1');
const tmp = fs.realpathSync(fs.mkdtempSync(path.join(process.platform === 'darwin' ? '/tmp' : os.tmpdir(), 'octocode-grammar-live-')));
const env = liveServerEnvironment({ ...process.env, OCTOCODE_HOME: path.join(tmp, 'home'), OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off',
  OCTOCODE_CLASSIFICATION_API: '', OCTOCODE_BETA: 'true', TMPDIR: tmp, TMP: tmp, TEMP: tmp,
}, ROOT);
configureRealServers(env.OCTOCODE_HOME);
const selected = new Set(process.argv.slice(2));
const report = { at: new Date().toISOString(), node: process.version, selected: [...selected], temporaryRoot: tmp, inventory: [], setup: [], results: [], calls: [],
  provenance: { contract: contract.fingerprint, native: nativeBinaryStamp() } };
const check = (name, ok, detail) => { report.results.push({ name, ok: !!ok, detail }); process.stdout.write(`${ok ? 'PASS' : 'FAIL'} ${name}\n`); };
const validators = contractValidators({ precompile: true });
const driver = createDriver({ validate: validators.validateOutput, validateInput: validators.validateExactInput,
  validateTransport: transportPagesProblem, tools: new Set(contract.tools.map(row => row.name)),
  schemaOf: tool => contract.tools.find(row => row.name === tool).outputSchema });
async function checkSemanticCase({ capability, ext, file, cwd, anchors }) {
for (const surface of ['cli', 'mcp']) for (const operation of ['definition', 'references']) {
  const label = `${capability.language}.${ext}.${surface}.${operation}`;
  const input = { queries: [{ path: file, workspaceRoot: cwd, operation, symbolName: anchors.symbol,
    line: anchors.position.line + 1, limit: 1, ...(operation === 'references' && { includeDeclaration: true }) }], responseChars: 300 };
  const settled = await settleSemanticQuery('lspSearch', input, async (seedTool, seedInput) => {
  const pages = [];
  const walked = await driver.explore(seedTool, seedInput, { surface, maxCalls: 1, call: async (tool, request) => {
    let result;
    if (surface === 'cli') {
      const run = await spawnCli([tool, JSON.stringify(request), '--json'], { env, cwd });
      let json; try { json = JSON.parse(run.stdout); } catch { /* The response assertion below retains invalid JSON as a failure. */ }
      result = { surface, json, sc: json, raw: json, stdout: run.stdout, text: run.stdout + run.stderr, status: run.status, ms: run.ms, isError: !json || run.status > 1 };
    } else {
      const response = await server.raw(tool, request, label, { keepRaw: true });
      result = { ...response, surface, json: response.sc };
    }
    pages.push(result);
    report.calls.push({ label, tool, surface, input: request, result });
    return result;
  } });
  return { pages, failures: walked.failures };
  });
  const locations = settled.pages.flatMap(page => lspLocations(page));
  const evidence = semanticLocationEvidence(locations, operation === 'definition' ? [anchors.target] : anchors.locations, file, surface === 'mcp' ? tmp : cwd);
  check(`${label}.complete`, !settled.failures.length && semanticPagesComplete(settled.pages), settled.failures.length ? settled.failures : settled.attempts.map(attempt => attempt.pages.map(page => page.json)));
  check(`${label}.exact-locations`, evidence.ok, evidence);
}
}

let server;
try {
  server = await startServer({ cwd: tmp, env, replaceEnv: true });
  const catalog = await spawnCli(['schema', '--view', 'full'], { env, cwd: tmp });
  const capabilities = JSON.parse(catalog.stdout).grammarCapabilities;
  const cases = selectedGrammarCases(capabilities, selected);
  report.expectedCases = cases.map(({ capability, ext }) => `${capability.language}.${ext}`);
  for (const { capability, ext } of cases) {
    try {
    const fixture = GRAMMAR_FIXTURES[capability.language];
    if (!fixture) { check(`${capability.language}.fixture`, false, 'Runtime grammar has no fixture'); continue; }
      const { cwd, file, fixture: projectFixture } = await createLiveProject({ tmp, capability, ext, fixture, env, report });
      (report.fixtures ??= []).push({ language: capability.language, extension: ext, file, source: projectFixture.src });
      const statusRun = await spawnCli(['lsp-server', 'status', file, '--json'], { env, cwd });
      const status = JSON.parse(statusRun.stdout);
      report.inventory.push({ language: capability.language, extension: ext, ...status });
      if (!status.serverAvailable) {
        check(`${capability.language}.${ext}.server`, false, status);
        continue;
      }
      const anchors = grammarAnchors(capability.language, projectFixture);
      await checkSemanticCase({ capability, ext, file, cwd, anchors });
    } catch (error) { check(`${capability.language}.${ext}.setup`, false, error.stack); }
  }
  check('coverage.all-selected-servers', report.inventory.length === cases.length && report.inventory.every(row => row.serverAvailable), { expected: report.expectedCases, inventory: report.inventory });
  report.providerVersions = await captureProviderVersions({ env, root: ROOT, cwd: tmp });
} catch (error) { check('harness', false, error.stack); }
finally {
  server?.close(); await stopTestBrokers(tmp);
  report.serverLogs = captureServerLogs(tmp, report.fixtures);
  if (process.env.OCTOCODE_LIVE_KEEP !== '1') fs.rmSync(tmp, { recursive: true, force: true });
}
report.provenance.stable = report.provenance.native.sha256 === nativeBinaryStamp()?.sha256;
check('provenance.stable', report.provenance.stable, report.provenance);
report.failed = report.results.filter(row => row.ok === false);
report.unavailable = report.inventory.filter(row => !row.serverAvailable);
writeResults('grammar-live', report);
process.stdout.write(`[grammar-live] ${report.results.filter(row => row.ok === true).length} passed; ${report.failed.length} failed; ${report.unavailable.length} unavailable; ${report.calls.length} calls\n`);
process.exitCode = report.failed.length ? 1 : 0;
