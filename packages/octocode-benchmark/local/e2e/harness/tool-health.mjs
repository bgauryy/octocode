// Tool health: a fast end-to-end check of every contract tool through BOTH the
// built CLI (`packages/octocode/out/octocode.js <tool> <json> --json`) and the
// MCP stdio server (`packages/octocode-mcp/dist/index.js`).
//
// Per tool (cases in harness/tool-health/cases.mjs):
//   happy        documented primary operation: exit 0, rows with the key fields,
//                output valid against the contract outputSchema
//   continuation the page lead (`next.<lead>`) walked to its end: every page has
//                rows, no row twice, union == the unpaged rows; extra walks
//                (`walks`) cover the tool's other page channels
//   explore      every seed (happy, page, empty, `explore`, `walks`) is explored
//                by tool-health/driver.mjs: every response contract-valid, every
//                typed continue/related call executed verbatim; opaque cursors
//                stay private and case-selected page rows reconstruct evidence
//   empty        exit 1, every row status "empty"
//   invalid      a typed contract error (errorCode in the contract), exit 2
//   parity       CLI and MCP return the same payload, status class and query graph
//   latency      observational only; --performance opts into wall-clock budgets
//
// Hermetic: a temp OCTOCODE_HOME/HOME, no user GitHub token, local fixtures
// under fixtures/tool-health (or a per-tool temp workspace built from it), and
// a loopback mock (tool-health/mock-server.mjs) for GitHub REST, the npm
// registry, git smart HTTP and the jev classifier. Every proxy variable points
// at a dead loopback port (NO_PROXY covers loopback), so any request that would
// leave the machine fails instead of passing silently. Each tool and surface
// has its own fake GitHub token and its own MCP server, so the runtime's
// per-credential search pacing never queues one tool behind another and every
// tool runs in parallel. Performance checks are separate from default correctness.
//
// Usage: node packages/octocode-benchmark/local/e2e/harness/tool-health.mjs [tool,...] [--deep] [--performance]
//   --deep  explore further: more non-page leads per seed and longer page walks
// Output: one line per tool, a summary line, results/tool-health.json; exit 1 on any FAIL.
import os from 'node:os';
import path from 'node:path';
import { FIXTURES, ROOT } from './paths.mjs';
import { writeResults } from './mcp-client.mjs';
import { contract, contractValidators } from './contract.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { CASES } from './tool-health/cases.mjs';
import { ADVANCED } from './tool-health/advanced.mjs';
import { createDriver } from './tool-health/driver.mjs';
import { transportPagesProblem } from './tool-health/compare.mjs';
import { runTool } from './tool-health/run-tool.mjs';
import { startSurfaces } from './tool-health/surfaces.mjs';
import { formatCheckSummary, summarizeChecks } from './tool-health/check-summary.mjs';

const started = performance.now();
const FIXTURE = path.join(FIXTURES, 'tool-health');
const args = process.argv.slice(2);
const DEEP = args.includes('--deep');
const PERFORMANCE = args.includes('--performance');
if (PERFORMANCE && process.env.OCTOCODE_LIVE !== '1') throw new Error('--performance requires OCTOCODE_LIVE=1; default tests use mocks and do not assert wall-clock latency');
const selected = args.find(a => !a.startsWith('--'))?.split(',');
const CONCURRENCY = 8;
const TOOLS = new Set(contract.tools.map(t => t.name));

// Compile before serving mocks so synchronous schema compilation cannot delay
// provider responses during tool calls.
const { validateOutput, validateInput, validateExactInput } = contractValidators({ precompile: true });

const surfaces = await startSurfaces(FIXTURE);
const { cliArgs } = surfaces;
const outputSchemas = new Map(contract.tools.map(tool => [tool.name, tool.outputSchema]));

// ---- run
const report = { at: new Date().toISOString(), deep: DEEP, performance: PERFORMANCE, environment: { platform: process.platform, arch: process.arch, node: process.version, kernel: os.release(), parallelism: os.availableParallelism(), services: process.env.OCTOCODE_LIVE === '1' ? 'loopback providers; real LSP integration' : 'hermetic loopback providers; mock LSP' }, tools: [], coverage: {} };
report.provenance = { contract: contract.fingerprint, native: nativeBinaryStamp() };
let exitCode = 0;
let surfaceServer;
try {
  // Coverage: every contract tool has a case, every case a contract tool.
  const contractTools = contract.tools.map(t => t.name);
  const noCase = contractTools.filter(t => !CASES[t]);
  const stale = Object.keys(CASES).filter(t => !contractTools.includes(t));
  const noAdvanced = contractTools.filter(t => !ADVANCED[t]?.length);
  const staleAdvanced = Object.keys(ADVANCED).filter(t => !contractTools.includes(t));
  report.coverage = { contractTools, noCase, stale, noAdvanced, staleAdvanced };
  if (noCase.length || stale.length || noAdvanced.length || staleAdvanced.length) { console.log(`FAIL coverage — ${JSON.stringify(report.coverage)}`); exitCode = 1; }

  // Surfaces list every contract tool: CLI schema (beta on) and MCP tools/list.
  const schema = await cliArgs(['schema']);
  surfaceServer = await surfaces.mcpServer('schema', FIXTURE);
  const cliTools = (() => { try { return JSON.parse(schema.stdout).tools.map(t => t.name); } catch { return []; } })();
  const mcpTools = surfaceServer.tools.map(t => t.name);
  const cliMissing = contractTools.filter(t => !cliTools.includes(t));
  const mcpMissing = contractTools.filter(t => !mcpTools.includes(t));
  const mcpExtra = mcpTools.filter(t => !contractTools.includes(t));
  report.surfaces = { cliTools, mcpTools, cliMissing, mcpMissing, mcpExtra };
  if (cliMissing.length || mcpMissing.length || mcpExtra.length) {
    console.log(`FAIL surfaces — CLI schema lacks: ${cliMissing.join(', ') || '-'}; MCP lacks: ${mcpMissing.join(', ') || '-'}; MCP exposes non-contract: ${mcpExtra.join(', ') || '-'}`);
    exitCode = 1;
  }

  const driver = createDriver({ validate: validateOutput, validateInput, validateTransport: transportPagesProblem, tools: TOOLS, schemaOf: tool => outputSchemas.get(tool) });
  surfaceServer.close();
  surfaceServer = null;
  const suite = { driver, surfaces, deep: DEEP, performance: PERFORMANCE, validateOutput, validateInput, validateExactInput, advanced: ADVANCED };

  // One job per tool: each tool and surface has its own credential, so none waits on another's pacing.
  const queue = contractTools.filter(t => CASES[t] && (!selected || selected.includes(t))).map(t => [t, CASES[t]]);
  // Longest first: tools with walks and LSP.
  queue.sort((a, b) => (b[1].budgetMs + (b[1].walks?.length ?? 0) * 5000) - (a[1].budgetMs + (a[1].walks?.length ?? 0) * 5000));
  const results = new Map();
  const workers = Array.from({ length: CONCURRENCY }, async () => {
    while (queue.length) {
      const [name, spec] = queue.shift();
      let result;
      try { result = await runTool(suite, name, spec); } catch (error) { result = { tool: name, status: 'FAIL', firstFailure: `harness: ${error.stack?.split('\n').slice(0, 2).join(' ') ?? error}`, errorStack: error.stack, checks: [], timings: [] }; }
      result.checkSummary = summarizeChecks(result.checks);
      results.set(name, result);
      const line = `${result.status.padEnd(4)} ${name.padEnd(17)} ${String(result.ms ?? 0).padStart(6)}ms  slowest ${String(result.slowestMs ?? 0).padStart(5)}/${String(result.budgetMs ?? '-').padEnd(5)}ms  ${String(result.calls ?? 0).padStart(3)} calls  checks: ${formatCheckSummary(result.checkSummary)}`;
      const failedCount = result.checkSummary.failed;
      const tail = result.status === 'FAIL' ? ` — ${result.firstFailure}${failedCount > 1 ? ` (+${failedCount - 1} more)` : ''}` : result.reason ? ` — ${result.reason}` : '';
      console.log(line + tail + (result.mockUnhandled ? ` [mock unhandled: ${result.mockUnhandled.slice(0, 2).join('; ')}]` : ''));
    }
  });
  await Promise.all(workers);
  report.tools = contractTools.filter(t => results.has(t)).map(t => results.get(t));
} catch (error) {
  console.log(`FAIL harness — ${error.stack ?? error}`);
  exitCode = 1;
} finally {
  surfaceServer?.close();
  await surfaces.close();
}
const count = s => report.tools.filter(t => t.status === s).length;
report.provenance.stable = !!report.provenance.native && report.provenance.native.sha256 === nativeBinaryStamp()?.sha256;
if (!report.provenance.stable) { console.log('FAIL provenance — native artifact missing or changed during the run'); exitCode = 1; }
if (count('FAIL')) {
  exitCode = 1;
  console.log('\nfailed checks:');
  for (const t of report.tools.filter(x => x.status === 'FAIL')) for (const c of t.checks.filter(x => x.ok === false)) console.log(`  ${t.tool} ${c.check}: ${c.detail}`);
}
report.seconds = Number(((performance.now() - started) / 1000).toFixed(1));
const distribution = values => {
  const sorted = values.toSorted((a, b) => a - b);
  const at = fraction => sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)] ?? null;
  return { min: sorted[0] ?? null, median: at(0.5), p95: at(0.95), max: sorted.at(-1) ?? null };
};
report.measurements = Object.fromEntries(['cli', 'mcp'].map(surface => {
  const calls = report.tools.flatMap(tool => tool.timings).filter(call => call.surface === surface);
  const bytes = calls.map(call => call.responseBytes).filter(Number.isFinite);
  return [surface, { calls: calls.length, latencyMs: distribution(calls.map(call => call.ms)), responseBytes: { measuredCalls: bytes.length, total: bytes.reduce((sum, value) => sum + value, 0), ...distribution(bytes) } }];
}));
report.measurementScope = 'Per-call elapsed time includes startup for CLI and uses a persistent MCP process per tool. responseBytes measures CLI stdout or the serialized MCP result/error body; it excludes JSON-RPC framing and stderr. Samples mix query shapes and concurrency; they are not transport speed comparisons.';
report.summary = { pass: count('PASS'), fail: count('FAIL'), skip: count('SKIP'), exitCode, checks: summarizeChecks(report.tools.flatMap(tool => tool.checks)) };
const file = writeResults('tool-health', report);
console.log(`\ntool-health${DEEP ? ' --deep' : ''}: ${count('PASS')} passed, ${count('FAIL')} failed, ${count('SKIP')} skipped of ${report.tools.length} tools, ${report.tools.reduce((n, t) => n + (t.calls ?? 0), 0)} calls in ${report.seconds}s; checks: ${formatCheckSummary(report.summary.checks)} — ${path.relative(ROOT, file)}`);
process.exitCode = exitCode;
