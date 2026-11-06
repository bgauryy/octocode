// A scenario provokes one named state through a public call: an error code, a partial
// reason, a lead channel or a refusal. Its files, environment and provider answers
// belong to the scenario. What it must show is compared after the walk ends.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { leadsIn } from '../tool-health/calls.mjs';

function writeFiles(dir, files = {}) {
  for (const [name, content] of Object.entries(files)) {
    const target = path.join(dir, name);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    if (content && content.symlink) fs.symlinkSync(content.symlink, target);
    else if (content && content.sparse) { fs.writeFileSync(target, content.head ?? ''); fs.truncateSync(target, content.sparse); }
    else fs.writeFileSync(target, content);
  }
}

/** A home whose language-server config runs the scripted server under the scenario's own name. */
function lspHome(dir, spec) {
  const home = path.join(dir, '.home');
  const config = path.join(home, '.octocode');
  fs.mkdirSync(config, { recursive: true });
  const scriptFile = path.join(dir, '.lsp-script.json');
  fs.writeFileSync(scriptFile, JSON.stringify(spec.script(pathToFileURL(fs.realpathSync(dir)).href)));
  const wrapper = path.join(dir, spec.command ?? 'matrix-server');
  fs.writeFileSync(wrapper, `#!/bin/sh\nexec "${process.execPath}" "${path.join(import.meta.dirname, 'mock-lsp-scripted.mjs')}"\n`);
  fs.chmodSync(wrapper, 0o755);
  const languages = spec.languages ?? { '.ts': 'typescript', '.tsx': 'typescriptreact', '.js': 'javascript', '.rs': 'rust' };
  const languageServers = Object.fromEntries(Object.entries(languages).map(([extension, languageId]) => [extension, { command: wrapper, args: [], languageId, env: { OCTOCODE_TEST_LSP_SCRIPT: scriptFile } }]));
  fs.writeFileSync(path.join(config, 'lsp-servers.json'), JSON.stringify({ languageServers }));
  return { OCTOCODE_HOME: config };
}

const withDirectory = (value, dir, mock) => JSON.parse(JSON.stringify(value).replaceAll('$DIR', dir).replaceAll('$MOCK', mock));

/** Everything a scenario's rows and lead walks emitted on one surface. */
function shown(pages, schemaOf) {
  const seen = { codes: new Set(), reasons: new Set(), channels: new Set(), text: [] };
  const visit = node => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(visit); return; }
    for (const [key, child] of Object.entries(node)) {
      if (key === 'errorCode' && typeof child === 'string') seen.codes.add(child);
      if (key === 'partialReasons' && Array.isArray(child)) child.forEach(reason => seen.reasons.add(reason));
      visit(child);
    }
  };
  for (const { tool, json } of pages) {
    visit(json);
    for (const lead of leadsIn(json, schemaOf(tool))) seen.channels.add(lead.key);
    seen.text.push(JSON.stringify(json));
  }
  return seen;
}

async function walkProblems(scenario, walk) {
  const report = await walk(scenario.tool, { queries: [scenario.query], ...(scenario.responseChars && { responseChars: scenario.responseChars }) });
  const lenient = new Set(scenario.allow ?? []);
  return report.failures.filter(failure => !lenient.has(failure.check)).map(failure => `${failure.check}: ${failure.detail}`);
}

const scenarioEnv = (scenario, context) => ({ ...scenario.env?.(context), ...(scenario.lsp && lspHome(context.dir, scenario.lsp)) });

/** What the scenario promised to show and did not. */
function absences(scenario, seen) {
  const missing = (label, names, found) => (names ?? []).filter(name => !found.has(name)).map(name => `${label} ${name} not shown: ${seen.text.join(' ').slice(0, 500)}`);
  return [...missing('code', scenario.codes, seen.codes), ...missing('reason', scenario.reasons, seen.reasons), ...missing('channel', scenario.channels, seen.channels)];
}

/** Run one scenario on one surface; a list of problems, empty when it held. */
export async function runScenario(worker, scenario, surface, ledger) {
  const dir = fs.mkdtempSync(path.join(worker.world.dir, 'sc-'));
  const rel = path.relative(worker.cwd, dir);
  const context = { ...worker.context, dir, rel, surface, worker };
  writeFiles(dir, typeof scenario.files === 'function' ? scenario.files(context) : scenario.files);
  scenario.setup?.(context);
  const env = scenarioEnv(scenario, context);
  const pages = [];
  const failures = [];
  const trace = process.env.OCTOCODE_MATRIX_TRACE;
  const answer = (scenario.github || trace) && (request => {
    const out = scenario.github?.(request, context) ?? null;
    if (trace) process.stderr.write(`trace ${scenario.id} ${surface} ${request.method ?? ''} ${request.p}${request.url?.search ?? ''} -> ${out ? out.status ?? 200 : 'model'}\n`);
    return out;
  });
  const provider = scenario.provider && ((request, headers) => scenario.provider(request, headers, context));
  worker.override = { github: answer, provider };
  const call = async (tool, input) => {
    const result = await worker.call(surface, tool, withDirectory(input, rel, worker.context.mock.url));
    pages.push({ tool, json: result.json, result });
    ledger.observe(worker, { surface, tool, result });
    return result;
  };
  const walk = async (tool, input) => {
    const own = [];
    const report = await worker.driver.explore(tool, withDirectory(input, rel, worker.context.mock.url), {
      surface, maxCalls: 1,
      call: async (name, request) => {
        const result = await worker.call(surface, name, request);
        const page = { tool: name, json: result.json, result };
        pages.push(page);
        own.push(page);
        ledger.observe(worker, { surface, tool: name, result });
        return result;
      },
    });
    return Object.assign(report, { pages: own });
  };
  try {
    await worker.withScenario(scenario, surface, env, async () => {
      if (scenario.run) {
        const problem = await scenario.run({ ...context, call, walk });
        if (problem) failures.push(problem);
      } else {
        failures.push(...await walkProblems(scenario, walk));
      }
    });
  } catch (error) {
    failures.push(`harness: ${error.message}`);
  } finally {
    worker.override = {};
    scenario.teardown?.(context);
  }
  failures.push(...absences(scenario, shown(pages, worker.schemaOf)));
  return failures.map(detail => ({ id: `scenario:${scenario.id}`, tool: scenario.tool ?? 'scenario', surface, check: 'scenario', detail, input: scenario.query ?? {} }));
}

