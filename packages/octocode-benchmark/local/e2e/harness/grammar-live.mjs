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
import {
  configureRealServers,
  createLiveProject,
  liveServerEnvironment,
  servedByClangd,
} from './grammar-live-projects.mjs';
import {
  finishedIndexProblems,
  indexedReferencesProblems,
  semanticLocationEvidence,
  selectedGrammarCases,
  unindexedReferencesProblems,
} from './grammar-live-oracle.mjs';
import { awaitProcessExit, removeLiveRoot } from './grammar-live-cleanup.mjs';
import {
  captureProviderVersions,
  captureServerLogs,
} from './grammar-live-evidence.mjs';
import {
  semanticPagesComplete,
  settleSemanticQuery,
} from './grammar-live-readiness.mjs';

if (process.env.OCTOCODE_LIVE !== '1')
  throw new Error('grammar-live requires OCTOCODE_LIVE=1');
const tmp = fs.realpathSync(
  fs.mkdtempSync(
    path.join(
      process.platform === 'darwin' ? '/tmp' : os.tmpdir(),
      'octocode-grammar-live-'
    )
  )
);
const env = liveServerEnvironment(
  {
    ...process.env,
    OCTOCODE_HOME: path.join(tmp, 'home'),
    OCTOCODE_LSP_AUTO_INSTALL: 'off',
    OCTOCODE_LSP_PREWARM: 'off',
    OCTOCODE_CLASSIFICATION_API: '',
    OCTOCODE_BETA: 'true',
    TMPDIR: tmp,
    TMP: tmp,
    TEMP: tmp,
  },
  ROOT
);
configureRealServers(env.OCTOCODE_HOME);
const selected = new Set(process.argv.slice(2));
const report = {
  at: new Date().toISOString(),
  node: process.version,
  selected: [...selected],
  temporaryRoot: tmp,
  inventory: [],
  setup: [],
  results: [],
  calls: [],
  provenance: { contract: contract.fingerprint, native: nativeBinaryStamp() },
};
const check = (name, ok, detail) => {
  report.results.push({ name, ok: !!ok, detail });
  process.stdout.write(`${ok ? 'PASS' : 'FAIL'} ${name}\n`);
};
const suiteDeadline =
  Date.now() + Number(process.env.OCTOCODE_TEST_SUITE_TIMEOUT_MS ?? 1_800_000);
const validators = contractValidators({ precompile: true });
const driver = createDriver({
  validate: validators.validateOutput,
  validateInput: validators.validateExactInput,
  validateTransport: transportPagesProblem,
  tools: new Set(contract.tools.map(row => row.name)),
  schemaOf: tool => contract.tools.find(row => row.name === tool).outputSchema,
});
/** Each row's coverage claim; every `references` row also keeps its exact-locations check below. */
function checkCoverage({ label, operation, settled, coverage, status }) {
  const receipts = settled.attempts.map(attempt =>
    attempt.pages.map(page => page.json)
  );
  const walked = settled.failures.length
    ? settled.failures.map(
        failure => `walk: ${failure.detail ?? failure.check}`
      )
    : [];
  if (operation === 'references' && coverage.unindexed) {
    const problems = [
      ...walked,
      ...unindexedReferencesProblems(settled.pages, coverage.unindexed),
    ];
    check(`${label}.honest-partial`, !problems.length, { problems, receipts });
    return;
  }
  if (operation === 'references' && coverage.database) {
    const problems = [
      ...walked,
      ...indexedReferencesProblems(settled.pages, coverage),
    ];
    check(`${label}.complete`, !problems.length, { problems, receipts });
    const index = finishedIndexProblems(status());
    check(`${label}.index-finished`, !index.length, index);
    return;
  }
  check(
    `${label}.complete`,
    !settled.failures.length && semanticPagesComplete(settled.pages),
    settled.failures.length ? settled.failures : receipts
  );
}

async function cliPage(tool, request, cwd) {
  const run = await spawnCli([tool, JSON.stringify(request), '--json'], {
    env,
    cwd,
  });
  let json;
  try {
    json = JSON.parse(run.stdout);
  } catch {
    /* The response assertion below retains invalid JSON as a failure. */
  }
  return {
    json,
    sc: json,
    raw: json,
    stdout: run.stdout,
    text: run.stdout + run.stderr,
    status: run.status,
    ms: run.ms,
    isError: !json || run.status > 1,
  };
}

async function mcpPage(tool, request, label) {
  const response = await server.raw(tool, request, label, { keepRaw: true });
  return { ...response, json: response.sc };
}

/** One attempt: every page the surface's answer takes, each kept as a receipt. */
function exploreSurface({ surface, label, cwd }) {
  return async (seedTool, seedInput) => {
    const pages = [];
    const walked = await driver.explore(seedTool, seedInput, {
      surface,
      maxCalls: 1,
      call: async (tool, request) => {
        const page =
          surface === 'cli'
            ? await cliPage(tool, request, cwd)
            : await mcpPage(tool, request, label);
        const result = { ...page, surface };
        pages.push(result);
        report.calls.push({ label, tool, surface, input: request, result });
        return result;
      },
    });
    return { pages, failures: walked.failures };
  };
}

async function checkSemanticCase({
  capability,
  ext,
  file,
  cwd,
  anchors,
  coverage,
  status,
}) {
  for (const surface of ['cli', 'mcp'])
    for (const operation of ['definition', 'references']) {
      const label = `${capability.language}.${ext}${coverage.database ? '.database' : ''}.${surface}.${operation}`;
      const input = {
        queries: [
          {
            path: file,
            workspaceRoot: cwd,
            operation,
            symbolName: anchors.symbol,
            line: anchors.position.line + 1,
            limit: 1,
            ...(operation === 'references' && { includeDeclaration: true }),
          },
        ],
        responseChars: 300,
      };
      const settled = await settleSemanticQuery(
        'lspSearch',
        input,
        exploreSurface({ surface, label, cwd }),
        coverage.database ? { reaskUntil: suiteDeadline } : {}
      );
      const locations = settled.pages.flatMap(page => lspLocations(page));
      const evidence = semanticLocationEvidence(
        locations,
        operation === 'definition' ? [anchors.target] : anchors.locations,
        file,
        surface === 'mcp' ? tmp : cwd
      );
      checkCoverage({ label, operation, settled, coverage, status });
      check(`${label}.exact-locations`, evidence.ok, evidence);
    }
}

/** The same source with a real compile_commands.json: `complete` must stay reachable. */
async function checkDatabaseCase({ capability, ext, fixture, anchors }) {
  const {
    cwd,
    file,
    fixture: source,
    coverage,
  } = await createLiveProject({
    tmp,
    capability,
    ext,
    fixture,
    env,
    report,
    compileDatabase: true,
  });
  (report.fixtures ??= []).push({
    language: capability.language,
    extension: ext,
    file,
    source: source.src,
    database: coverage.database,
  });
  const statusRun = await spawnCli(['lsp-server', 'status', file, '--json'], {
    env,
    cwd,
  });
  const status = JSON.parse(statusRun.stdout);
  await checkSemanticCase({
    capability,
    ext,
    file,
    cwd,
    anchors,
    coverage,
    status: () => ({ status, home: env.OCTOCODE_HOME, cwd, file }),
  });
}

let server;
try {
  server = await startServer({ cwd: tmp, env, replaceEnv: true });
  const catalog = await spawnCli(['schema', '--view', 'full'], {
    env,
    cwd: tmp,
  });
  const capabilities = JSON.parse(catalog.stdout).grammarCapabilities;
  const cases = selectedGrammarCases(capabilities, selected);
  report.expectedCases = cases.map(
    ({ capability, ext }) => `${capability.language}.${ext}`
  );
  for (const { capability, ext } of cases) {
    try {
      const fixture = GRAMMAR_FIXTURES[capability.language];
      if (!fixture) {
        check(
          `${capability.language}.fixture`,
          false,
          'Runtime grammar has no fixture'
        );
        continue;
      }
      const {
        cwd,
        file,
        fixture: projectFixture,
        coverage,
      } = await createLiveProject({
        tmp,
        capability,
        ext,
        fixture,
        env,
        report,
      });
      (report.fixtures ??= []).push({
        language: capability.language,
        extension: ext,
        file,
        source: projectFixture.src,
      });
      const statusRun = await spawnCli(
        ['lsp-server', 'status', file, '--json'],
        { env, cwd }
      );
      const status = JSON.parse(statusRun.stdout);
      report.inventory.push({
        language: capability.language,
        extension: ext,
        ...status,
      });
      if (!status.serverAvailable) {
        process.stdout.write(
          `SKIP [grammar-live] ${capability.language}.${ext} — no language server: ${status.probeError ?? 'none installed'}, see grammar-live-setup.md\n`
        );
        continue;
      }
      const anchors = grammarAnchors(capability.language, projectFixture);
      await checkSemanticCase({
        capability,
        ext,
        file,
        cwd,
        anchors,
        coverage,
      });
      if (servedByClangd(capability.language) && ext === fixture.ext)
        await checkDatabaseCase({ capability, ext, fixture, anchors });
    } catch (error) {
      check(`${capability.language}.${ext}.setup`, false, error.stack);
    }
  }
  check(
    'coverage.all-selected-servers',
    report.inventory.length === cases.length,
    { expected: report.expectedCases, inventory: report.inventory }
  );
  report.providerVersions = await captureProviderVersions({
    env,
    root: ROOT,
    cwd: tmp,
  });
} catch (error) {
  check('harness', false, error.stack);
} finally {
  server?.close();
  await stopTestBrokers(tmp);
  report.stoppedAt = Date.now();
  report.serverLogs = captureServerLogs(tmp, report.fixtures);
}
report.provenance.stable =
  report.provenance.native.sha256 === nativeBinaryStamp()?.sha256;
check('provenance.stable', report.provenance.stable, report.provenance);
report.failed = report.results.filter(row => row.ok === false);
report.unavailable = report.inventory.filter(row => !row.serverAvailable);
writeResults('grammar-live', report);
process.stdout.write(
  `[grammar-live] ${report.results.filter(row => row.ok === true).length} passed; ${report.failed.length} failed; ${report.unavailable.length} unavailable; ${report.calls.length} calls\n`
);
process.exitCode = report.failed.length ? 1 : 0;
// The results are on disk: a directory a server still holds costs a warning, never the run.
report.processExit = await awaitProcessExit(tmp);
if (report.processExit.stillRunning.length)
  process.stderr.write(
    `[grammar-live] warning: still running ${report.processExit.waitedMs} ms after the brokers stopped: ${report.processExit.stillRunning.map(row => `${row.pid} ${row.command.slice(0, 100)}`).join('; ')}\n`
  );
if (process.env.OCTOCODE_LIVE_KEEP !== '1') {
  report.cleanup = removeLiveRoot(tmp, { since: report.stoppedAt });
  if (!report.cleanup.removed)
    process.stderr.write(
      `[grammar-live] warning: ${tmp} was not removed: ${report.cleanup.error}; held by ${report.cleanup.holders.map(row => row.pid).join(', ') || 'no process found'}; ${report.cleanup.writtenSinceStop.length} files changed after the stop\n`
    );
}
writeResults('grammar-live', report);
