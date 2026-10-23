import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {
  ROOT,
  FIXTURES,
  expandShared,
  hermeticEnv,
  startServer,
  structureFiles,
} from '../mcp-client.mjs';
import { startMock } from '../tool-health/mock-server.mjs';

export { ROOT, startServer };
export const contract = JSON.parse(
  fs.readFileSync(
    path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'),
    'utf8'
  )
);
export const PAGE_CHANNELS = new Set(contract.continuationChannels.kinds.pages);
export const canon = value =>
  Array.isArray(value)
    ? value.map(canon)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map(key => [key, canon(value[key])])
        )
      : value;
export const stable = value => JSON.stringify(canon(value));
export const dataOf = result => result.json?.results?.[0]?.data;
export function success(result) {
  assert.ok(
    result.json?.results?.length,
    `missing rows: ${JSON.stringify(result.json)}`
  );
  assert.notEqual(
    result.json.results[0].status,
    'error',
    JSON.stringify(result.json)
  );
  return dataOf(result);
}

export function failure(result, code) {
  const row = result.json?.results?.[0];
  assert.equal(row?.status, 'error', JSON.stringify(result.json));
  assert.equal(row.data?.errorCode, code, JSON.stringify(result.json));
  if (result.code !== undefined)
    assert.equal(result.code, contract.errorCodes[code] === 'rateLimited' ? 7 : 5);
  if (result.isError !== undefined) assert.equal(result.isError, true);
  return row.data;
}

export function answer(request, mode = 'score') {
  const state = JSON.stringify(request.state);
  const high =
    /SELECTED_SELECTION_SENTINEL|Cancellation removes|greeter\.ts|demo-docs|tiny-pulse|Add greeter module/.test(
      state
    );
  const uncertain = /UNKNOWN_SELECTION_SENTINEL/.test(state);
  const entries = Object.entries(request.questions ?? {});
  const answers = Object.fromEntries(
    entries.map(([id, q], index) => {
      if (q.type === 'noul')
        return [id, { type: 'noul', noul: high ? 0.99 : 0.01 }];
      assert.equal(q.type, 'score');
      assert.equal(
        q.criteria.length,
        3,
        'selection uses the three-level relevance policy'
      );
      let probabilities;
      if (mode === 'policyAnyDirect' || mode === 'invalidContradiction')
        probabilities =
          index === 0 ? { 0: 0.49, 1: 0, 2: 0.51 } : { 0: 1, 1: 0, 2: 0 };
      else if (mode === 'policyAllNo')
        probabilities = { 0: 0.51, 1: 0.49, 2: 0 };
      else if (mode === 'policyUnknown')
        probabilities =
          index === 0 ? { 0: 0.5, 1: 0.5, 2: 0 } : { 0: 0.9, 1: 0.1, 2: 0 };
      else if (uncertain) probabilities = { 0: 0.5, 1: 0.5, 2: 0 }; // no level passes the threshold: unknown (a confident level 1 is related)
      else if (high) probabilities = { 0: 0, 1: 0, 2: 1 };
      else probabilities = { 0: 1, 1: 0, 2: 0 };
      const expectedScore = Object.entries(probabilities).reduce(
        (sum, [level, probability]) =>
          sum + Number(level) * Number(probability),
        0
      );
      const score =
        mode === 'invalidContradiction' && index === 0 ? 2 : expectedScore;
      return [
        id,
        {
          type: 'score',
          score,
          confidence: 1,
          probabilities,
          legend: Object.fromEntries(
            q.criteria.map((value, i) => [String(i), value])
          ),
        },
      ];
    })
  );
  return {
    model: request.model,
    answers,
    usage: { input_tokens: 10, output_tokens: 2 },
  };
}

export async function setup({
  classificationResponder,
  label = 'acceptance',
} = {}) {
  const scratch = path.join(ROOT, '.octocode/tmp/read-tool-selection-eval');
  fs.mkdirSync(scratch, { recursive: true });
  const fixture = fs.mkdtempSync(path.join(scratch, `${label}-`));
  fs.cpSync(path.join(FIXTURES, 'tool-health'), fixture, { recursive: true });
  for (const name of ['z-extra-a', 'z-extra-b']) {
    fs.writeFileSync(
      path.join(fixture, 'src', `${name}.ts`),
      `import { greet } from './greeter';\n// healthMarker additional caller\nexport function extra(): string {\n  return greet('first').text + greet('second').text;\n}\n`
    );
  }
  // A deterministic multi-window document with selected, excluded and unknown
  // candidates. No answer labels are sent to the implementation under test.
  const records = Array.from(
    { length: 80 },
    (_, index) =>
      `Record ${index + 1}: background bookkeeping retains an audit entry for the ordinary request lifecycle.`
  );
  records[4] =
    'SELECTED_SELECTION_SENTINEL Cancellation removes a queued request before any work starts.';
  records[24] =
    'UNKNOWN_SELECTION_SENTINEL The relationship to cancellation is not established.';
  records[44] =
    'EXCLUDED_SELECTION_SENTINEL Routine bookkeeping does not cancel queued work.';
  records[75] =
    'SELECTED_SELECTION_SENTINEL Cancellation removes another queued request before work starts.';
  fs.writeFileSync(
    path.join(fixture, 'selection-guide.txt'),
    `${records.join('\n')}\n`
  );
  fs.writeFileSync(
    path.join(fixture, 'single-selected.txt'),
    'SELECTED_SELECTION_SENTINEL Cancellation removes a queued request before any work starts.\n'
  );
  fs.writeFileSync(
    path.join(fixture, 'src', 'unknown-selection.ts'),
    '// healthMarker UNKNOWN_SELECTION_SENTINEL\nexport const uncertain = true;\n'
  );
  fs.writeFileSync(
    path.join(fixture, 'src', 'selected-selection.ts'),
    '// healthMarker SELECTED_SELECTION_SENTINEL\nexport const selected = "this.prefix";\n'
  );
  // Outside each queried source selection: provider packets must never gain it.
  fs.writeFileSync(
    path.join(fixture, 'not-selected.txt'),
    'BODY_ONLY_SENTINEL private fixture body outside the selected evidence.\n'
  );
  const state = { mode: 'score' };
  const classificationHeaders = [];
  const mock = await startMock({
    fixtureDir: fixture,
    token: 'fixture-token',
    classificationResponder: async (request, headers) => {
      classificationHeaders.push(headers);
      if (classificationResponder)
        return classificationResponder(request, headers);
      if (state.mode === 'timeout')
        await new Promise(resolve => setTimeout(resolve, 350));
      if (['401', '429', '500'].includes(state.mode))
        return {
          status: Number(state.mode),
          body: { error: 'synthetic provider failure' },
        };
      if (state.mode === 'malformed') return { body: '{broken' };
      return { body: answer(request, state.mode) };
    },
  });
  // Stop repository ignore discovery at this owned fixture, preserving default ignore semantics.
  // Initialize after the mock snapshots source files so Git metadata is never remote evidence.
  const initialized = spawnSync('git', ['init', '--quiet', fixture], {
    encoding: 'utf8',
  });
  assert.equal(initialized.status, 0, initialized.stderr);
  const isolated = hermeticEnv(process.env, `read-selection-${label}`);
  const trace = [];
  const env = {
    ...isolated,
    GITHUB_TOKEN: 'fixture-token',
    GITHUB_API_URL: `${mock.url}/api/v3`,
    OCTOCODE_CLASSIFICATION_API: 'fixture-key',
    OCTOCODE_CLASSIFICATION_API_HOST: mock.url,
    OCTOCODE_STORAGE_MODE: 'memory',
    OCTOCODE_ALLOW_PRIVATE_REGISTRY: 'true',
    OCTOCODE_ENABLE_LOCAL: 'true',
    OCTOCODE_BETA: 'true',
    OCTOCODE_LSP_AUTO_INSTALL: 'off',
    OCTOCODE_LSP_PREWARM: 'off',
    DISABLE_TOOLS: '',
    TOOLS_TO_RUN: '',
    ALLOWED_PATHS: fixture,
    WORKSPACE_ROOT: fixture,
  };
  const cliArgs = (args, overrides = {}) =>
    new Promise((resolve, reject) => {
      const started = performance.now();
      const child = spawn(
        process.execPath,
        [path.join(ROOT, 'packages/octocode/out/octocode.js'), ...args],
        { cwd: fixture, env: { ...env, ...overrides } }
      );
      let stdout = '',
        stderr = '';
      child.stdout.on('data', chunk => {
        stdout += chunk;
      });
      child.stderr.on('data', chunk => {
        stderr += chunk;
      });
      const timer = setTimeout(() => child.kill('SIGKILL'), 60000);
      child.on('error', error => {
        clearTimeout(timer);
        reject(error);
      });
      child.on('close', code => {
        clearTimeout(timer);
        try {
          const result = {
            json: expandShared(JSON.parse(stdout)),
            rawResponse: stdout,
            stderr,
            code,
            ms: performance.now() - started,
          };
          trace.push({ args, overrides, ...result });
          resolve(result);
        } catch {
          reject(
            new Error(
              `CLI ${code}: ${stderr.slice(0, 500)} ${stdout.slice(0, 500)}`
            )
          );
        }
      });
    });
  const cli = (tool, query, overrides) =>
    cliArgs([tool, JSON.stringify({ queries: [query] }), '--json'], overrides);
  const close = async () => {
    await mock.close();
    fs.rmSync(fixture, { recursive: true, force: true });
    fs.rmSync(isolated.OCTOCODE_HOME, { recursive: true, force: true });
  };
  return {
    fixture,
    env,
    mock,
    state,
    cli,
    cliArgs,
    close,
    trace,
    classificationHeaders,
    calls: () => mock.classificationRequests.length,
  };
}

/** Only navigation/measurement metadata is excluded. Evidence payloads remain. */
export function evidenceValue(value) {
  if (Array.isArray(value)) return value.map(evidenceValue);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(
        ([key]) =>
          ![
            'next',
            'hints',
            'selection',
            'pagination',
            'snapshot',
            'timing',
            'durationMs',
          ].includes(key)
      )
      .map(([key, child]) => [key, evidenceValue(child)])
  );
}

export function evidence(spec, data, query = {}) {
  if (spec.kind === 'source') {
    const lines = String(data.content ?? '')
      .split('\n')
      .filter(line => /^\d+\t/.test(line));
    return lines.map(line =>
      stable({
        path: data.path,
        revision:
          data.commitSha ??
          (/^[a-f0-9]{40}$/i.test(query.ref ?? '') ? query.ref : undefined),
        line,
      })
    );
  }
  if (spec.kind === 'structure')
    return structureFiles(data.files).map(row => stable(evidenceValue(row)));
  if (spec.kind === 'files') {
    if (Array.isArray(data.files))
      return data.files.flatMap(file => {
        const value = evidenceValue(file);
        // Symbol pagination may divide one file group across pages. Preserve every
        // symbol payload and its file identity while comparing the complete walk.
        if (spec.tool === 'localSearch' && Array.isArray(value.matches)) {
          const { matches, ...identity } = value;
          return matches.map(match => stable({ ...identity, match }));
        }
        if (spec.tool === 'astSearch' && Array.isArray(value.symbols)) {
          const { symbols, ...identity } = value;
          return symbols.map(symbol => stable({ ...identity, symbol }));
        }
        return [stable(value)];
      });
    if (Array.isArray(data.matches))
      return data.matches.map(row => stable(evidenceValue(row)));
    if (Array.isArray(data.symbols))
      return data.symbols.map(row => stable(evidenceValue(row)));
    return [];
  }
  if (spec.kind !== 'structural')
    return (data[spec.kind] ?? []).map(row => stable(evidenceValue(row)));
  return [stable(evidenceValue(data))];
}

/** Every executable page edge at any nesting level, excluding optional hints. */
export function pages(value) {
  const out = [];
  const visit = node => {
    if (!node || typeof node !== 'object') return;
    for (const [key, child] of Object.entries(node)) {
      if (key === 'hints') continue;
      if (key === 'next' && child && typeof child === 'object') {
        for (const [kind, lead] of Object.entries(child)) {
          if (PAGE_CHANNELS.has(kind.replace(/\d+$/, '')) && lead?.tool && lead.query)
            out.push({ kind, ...lead });
        }
      } else visit(child);
    }
  };
  visit(value);
  return out;
}

async function walkLeads(
  ctx,
  spec,
  initial,
  {
    surface = 'cli',
    mcp,
    overrides = {},
    maxPages = 80,
    include = lead => !lead.kind.startsWith('unfiltered'),
    errorCode,
  } = {}
) {
  const pending = initial.map(lead => ({ ...lead }));
  const seen = new Set();
  const responses = [],
    rows = [],
    edges = [],
    provider = [];
  while (pending.length) {
    assert.ok(responses.length < maxPages, 'continuation walk must terminate');
    const lead = pending.shift(),
      identity = stable(lead);
    assert.ok(!seen.has(identity), `repeated continuation edge: ${identity}`);
    seen.add(identity);
    assert.equal(
      lead.tool,
      spec.tool,
      'page continuation stays on its evidence tool'
    );
    const before = ctx.calls();
    let result;
    if (surface === 'mcp') {
      const raw = await mcp.raw(lead.tool, lead.query);
      result = {
        json: raw.sc,
        rawResponse: raw.text,
        ms: raw.ms,
        isError: raw.isError,
      };
    } else
      result = await ctx.cliArgs(
        [lead.tool, JSON.stringify(lead.query), '--json'],
        overrides
      );
    const data = errorCode ? failure(result, errorCode) : success(result);
    provider.push(ctx.calls() - before);
    responses.push({ input: lead.query, ...result });
    if (spec.tool === 'ghGetFileContent')
      assert.match(
        data.commitSha ?? lead.query.queries[0].ref ?? '',
        /^[a-f0-9]{40}$/i,
        'remote source retains resolved revision in output or pinned input'
      );
    rows.push(...evidence(spec, data, lead.query.queries[0]));
    const next = pages(data).filter(include);
    for (const nextLead of next) {
      assert.ok(
        Array.isArray(nextLead.query.queries),
        'page lead has a complete input envelope'
      );
      edges.push(nextLead);
      pending.push({ tool: nextLead.tool, query: nextLead.query });
    }
  }
  return {
    rows,
    responses,
    edges,
    provider,
    providerCalls: provider.reduce((a, b) => a + b, 0),
  };
}

export async function walk(ctx, spec, query, options = {}) {
  return walkLeads(
    ctx,
    spec,
    [{ tool: spec.tool, query: { queries: [query] } }],
    options
  );
}

export async function recoverUnfiltered(ctx, spec, response, options = {}) {
  const leads = pages(dataOf(response));
  const lead = leads.find(row => row.kind === 'unfiltered');
  assert.ok(lead, 'classified output exposes next.unfiltered');
  const before = ctx.calls();
  const run = await walkLeads(
    ctx,
    spec,
    [{ tool: lead.tool, query: lead.query }],
    options
  );
  assert.equal(
    ctx.calls(),
    before,
    'unfiltered recovery performs no JEV calls'
  );
  for (const page of run.responses) {
    const query = page.input.queries[0];
    assert.equal(
      query.clasifyContext,
      undefined,
      'recovery removes classification context'
    );
    assert.equal(
      query.selection,
      undefined,
      'recovery removes the selection token'
    );
  }
  return run;
}

export function fieldsInEveryBranch(schema, field) {
  if (schema.properties) return field in schema.properties;
  const branches = schema.oneOf ?? schema.anyOf;
  return (
    !!branches?.length &&
    branches.every(branch => fieldsInEveryBranch(branch, field))
  );
}
