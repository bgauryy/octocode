// Every runtime grammar and owned extension, both transports, complete page walks.
import fs from 'node:fs';
import path from 'node:path';
import { FIXTURES } from './paths.mjs';
import { writeResults } from './mcp-client.mjs';
import { contract, contractValidators } from './contract.mjs';
import { nativeBinaryStamp } from './native-binary.mjs';
import { startSurfaces } from './tool-health/surfaces.mjs';
import { createDriver } from './tool-health/driver.mjs';
import { transportPagesProblem } from './tool-health/compare.mjs';
import { GRAMMAR_FIXTURES } from './grammar-fixtures.mjs';
import { configureGrammarLsp } from './grammar-lsp.mjs';
import { runGrammarCase } from './grammar-cases.mjs';

const report = { at: new Date().toISOString(), node: process.version, results: [], cases: [], calls: [],
  scope: 'Native grammar and transport behavior; LSP uses a strict protocol fixture, not semantic language servers.',
  provenance: { contract: contract.fingerprint, native: nativeBinaryStamp() } };
const check = (name, ok, detail) => {
  report.results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${ok ? '' : `: ${JSON.stringify(detail)}`}`);
};
const validators = contractValidators({ precompile: true });
const schemaOf = tool => contract.tools.find(row => row.name === tool).outputSchema;
const driver = createDriver({ validate: validators.validateOutput, validateInput: validators.validateExactInput,
  validateTransport: transportPagesProblem, tools: new Set(contract.tools.map(row => row.name)), schemaOf });
const surfaces = await startSurfaces(path.join(FIXTURES, 'tool-health'));
let server;
try {
  const inventory = (await surfaces.cliArgs(['schema', '--view', 'full'])).json.grammarCapabilities;
  report.inventory = inventory;
  check('inventory.languages', JSON.stringify(inventory.map(row => row.language).toSorted()) === JSON.stringify(Object.keys(GRAMMAR_FIXTURES).toSorted()), inventory.map(row => row.language));
  const cases = [];
  const cwd = fs.realpathSync(fs.mkdtempSync(path.join(surfaces.ctx.tmp, 'grammar-')));
  for (const capability of inventory) {
    const fixture = GRAMMAR_FIXTURES[capability.language];
    if (!fixture) continue;
    check(`${capability.language}.extensions`, JSON.stringify([fixture.ext, ...fixture.alt].toSorted((a, b) => a.localeCompare(b))) === JSON.stringify(capability.extensions.toSorted((a, b) => a.localeCompare(b))), capability.extensions);
    const directory = path.join(cwd, fixture.ext);
    fs.mkdirSync(directory);
    for (const ext of capability.extensions) {
      const file = path.join(directory, `main.${ext}`);
      fs.writeFileSync(file, fixture.src);
      cases.push({ language: capability.language, capability, fixture, ext, file });
    }
  }
  const traceFile = configureGrammarLsp(path.join(surfaces.ctx.tmp, 'home'), cases);
  server = await surfaces.mcpServer('grammar', cwd);
  const walk = async (tool, input, surface, label) => {
    const pages = [];
    check(`${label}.input`, !validators.validateExactInput(tool, input), validators.validateExactInput(tool, input));
    const walkReport = await driver.explore(tool, input, { surface, maxCalls: 1, call: async (name, request) => {
      const result = surface === 'cli' ? await surfaces.cliCall(name, request, { cwd }) : await surfaces.mcpCall(server, name, request);
      pages.push(result);
      report.calls.push({ label, tool: name, surface, input: request, result });
      return result;
    } });
    check(`${label}.complete`, !walkReport.failures.length && pages.every(page => !page.isError && !page.json?.results?.some(row => row.status === 'error')), walkReport.failures.length ? walkReport.failures : pages.filter(page => page.isError).map(page => page.json));
    return pages;
  };
  for (const entry of cases) {
    const outcomes = {};
    for (const surface of ['cli', 'mcp']) outcomes[surface] = await runGrammarCase(entry, surface, walk, check);
    check(`${entry.language}.${entry.ext}.transport-parity`, JSON.stringify(outcomes.cli) === JSON.stringify(outcomes.mcp), outcomes);
    report.cases.push({ language: entry.language, extension: entry.ext, outcomes });
    console.log(`CHECKED ${entry.language} .${entry.ext} on CLI + MCP`);
  }
  report.lspTrace = fs.existsSync(traceFile) ? fs.readFileSync(traceFile, 'utf8').trim().split('\n').map(line => JSON.parse(line)) : [];
  for (const entry of cases) {
    const opened = report.lspTrace.filter(row => row.method === 'textDocument/didOpen' && new URL(row.params.textDocument.uri).pathname === entry.file);
    const languageId = entry.ext === 'jsx' ? 'javascriptreact' : entry.capability.languageId;
    check(`${entry.language}.${entry.ext}.lsp.language-id`, opened.length > 0 && opened.every(row => row.params.textDocument.languageId === languageId), opened.map(row => row.params.textDocument.languageId));
  }
} catch (error) { check('harness', false, error.stack); }
finally { server?.close(); await surfaces.close(); }
report.provenance.stable = report.provenance.native.sha256 === nativeBinaryStamp()?.sha256;
check('provenance.stable', report.provenance.stable, report.provenance);
report.failed = report.results.filter(row => !row.ok);
writeResults('grammar', report);
console.log(`[grammar] ${report.results.length - report.failed.length}/${report.results.length} passed; ${report.cases.length} extensions; ${report.calls.length} calls`);
process.exitCode = report.failed.length ? 1 : 0;
