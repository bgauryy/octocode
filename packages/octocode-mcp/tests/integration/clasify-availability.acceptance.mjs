/** Key-based clasify and reader-selection availability in built CLI and fresh MCP hosts. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { createAjv } from '../../../octocode-benchmark/local/e2e/harness/query-defaults.mjs';
const ajv = createAjv();

const root = process.cwd();
const base = path.join(root, '.octocode/octocode-dev/clasify-availability');
const cliPath = path.join(root, 'packages/octocode/out/octocode.js');
const mcpPath = path.join(root, 'packages/octocode-mcp/dist/index.js');
const selectionTools = new Set([
  'localFetch',
  'localSearch',
  'ghGetFileContent',
  'ghSearchCode',
]);
await fs.mkdir(base, { recursive: true });
const fixture = await fs.mkdtemp(path.join(base, 'fixture-'));

function cli(project, env, args) {
  const result = spawnSync(process.execPath, [cliPath, ...args], {
    env,
    cwd: project,
    encoding: 'utf8',
    timeout: 15000,
  });
  return {
    ...result,
    json: (() => {
      try {
        return JSON.parse(result.stdout);
      } catch {
        return undefined;
      }
    })(),
  };
}

function schema(project, env, args) {
  const result = cli(project, env, ['schema', ...args]);
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.stderr}`);
  return result.json;
}

const selection = { predicates: ['Does this contain selected evidence?'] };
const queryFor = name =>
  ({
    localFetch: { path: 'evidence.txt' },
    localSearch: { path: '.', matchString: ['SELECTED'] },
    ghGetFileContent: { owner: 'octo', repo: 'demo', path: 'evidence.txt' },
    ghSearchCode: { owner: 'octo', repo: 'demo', keywords: ['selected'] },
  })[name];

function checkAdmission(schema, row, { enabled, envelope, label }) {
  const validate = ajv.compile(schema);
  const input = query => (envelope ? { queries: [query] } : query);
  assert.equal(validate(input(row)), true, `${label}: ordinary read admitted`);
  assert.equal(
    validate(input({ ...row, classification: selection })),
    enabled,
    `${label}: classification capability`
  );
  assert.equal(
    validate(input({ ...row, selection: 'caller-state' })),
    false,
    `${label}: private state rejected`
  );
}

const cases = [
  { name: 'absent', context: false, clasify: false },
  {
    name: 'empty',
    env: { OCTOCODE_CLASSIFICATION_API: '' },
    context: false,
    clasify: false,
  },
  {
    name: 'whitespace',
    env: { OCTOCODE_CLASSIFICATION_API: '   ' },
    context: false,
    clasify: false,
  },
  {
    name: 'process-key-beta-off',
    env: { OCTOCODE_CLASSIFICATION_API: 'fixture-key', OCTOCODE_BETA: 'false' },
    context: true,
    clasify: false,
  },
  {
    name: 'process-key-beta-on',
    env: { OCTOCODE_CLASSIFICATION_API: 'fixture-key', OCTOCODE_BETA: 'true' },
    context: true,
    clasify: true,
  },
  {
    name: 'hidden-public-clasify-keeps-context',
    env: {
      OCTOCODE_CLASSIFICATION_API: 'fixture-key',
      OCTOCODE_BETA: 'true',
      DISABLE_TOOLS: 'clasify',
    },
    context: true,
    clasify: false,
  },
  {
    name: 'home-env-key',
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-key\nOCTOCODE_BETA=true\n',
    context: true,
    clasify: true,
  },
  {
    name: 'project-env-key',
    projectEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-key\n',
    context: true,
    clasify: false,
  },
  {
    name: 'config-key',
    config: { classification: { api: 'fixture-key' }, local: { beta: true } },
    context: true,
    clasify: true,
  },
  {
    name: 'process-empty-overrides-home-and-config',
    env: { OCTOCODE_CLASSIFICATION_API: '' },
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    config: { classification: { api: 'fixture-config' } },
    context: false,
    clasify: false,
  },
  {
    name: 'key-with-no-classification-route',
    env: {
      OCTOCODE_CLASSIFICATION_API: 'fixture-key',
      OCTOCODE_BETA: 'true',
      TOOLS_TO_RUN: 'ghSearchHistory',
    },
    context: false,
    clasify: false,
    readers: false,
  },
  {
    name: 'standalone-only',
    env: {
      OCTOCODE_CLASSIFICATION_API: 'fixture-key',
      OCTOCODE_BETA: 'true',
      TOOLS_TO_RUN: 'clasify',
    },
    context: false,
    clasify: true,
    readers: false,
  },
  {
    name: 'retired-jev-key-is-not-a-capability',
    homeEnv: 'OCTOCODE_JEV_KEY=fixture-key\n',
    context: false,
    clasify: false,
  },
];

const results = [];
try {
  for (const row of cases) {
    const home = path.join(fixture, row.name, 'home');
    const project = path.join(fixture, row.name, 'project');
    await fs.mkdir(home, { recursive: true });
    await fs.mkdir(path.join(project, '.octocode'), { recursive: true });
    await fs.writeFile(
      path.join(project, 'evidence.txt'),
      'SELECTED evidence\n'
    );
    if (row.homeEnv) await fs.writeFile(path.join(home, '.env'), row.homeEnv);
    if (row.projectEnv)
      await fs.writeFile(path.join(project, '.octocode/.env'), row.projectEnv);
    if (row.config)
      await fs.writeFile(
        path.join(home, '.octocoderc'),
        JSON.stringify(row.config)
      );
    const env = {
      PATH: process.env.PATH ?? '',
      HOME: path.join(fixture, row.name),
      OCTOCODE_HOME: home,
      WORKSPACE_ROOT: project,
      ALLOWED_PATHS: project,
      OCTOCODE_STORAGE_MODE: 'memory',
      OCTOCODE_CLASSIFICATION_API_HOST: 'http://127.0.0.1:9',
      DISABLE_TOOLS: '',
      TOOLS_TO_RUN: '',
      ...row.env,
    };
    const client = new Client({ name: 'clasify-availability', version: '1' });
    let stderr = '';
    try {
      const transport = new StdioClientTransport({
        command: process.execPath,
        args: [mcpPath],
        cwd: project,
        env,
        stderr: 'pipe',
      });
      transport.stderr?.on('data', chunk => {
        stderr += chunk;
      });
      await client.connect(transport);
      const catalog = await client.listTools();
      assert.equal(
        catalog.tools.some(tool => tool.name === 'clasify'),
        row.clasify,
        `${row.name}: MCP clasify`
      );
      const cliCatalog = schema(project, env, ['--view', 'full']);
      assert.equal(
        cliCatalog.tools.some(tool => tool.name === 'clasify'),
        row.clasify,
        `${row.name}: CLI clasify`
      );
      for (const name of selectionTools) {
        const mcpTool = catalog.tools.find(tool => tool.name === name);
        if (row.readers === false) {
          assert.equal(mcpTool, undefined, `${row.name}: ${name} unavailable`);
          continue;
        }
        assert.ok(mcpTool, `${row.name}: MCP ${name}`);
        checkAdmission(mcpTool.inputSchema, queryFor(name), {
          enabled: row.context,
          envelope: true,
          label: `${row.name}: MCP ${name}`,
        });
        checkAdmission(
          schema(project, env, [name]).querySchema,
          queryFor(name),
          {
            enabled: row.context,
            envelope: false,
            label: `${row.name}: CLI ${name}`,
          }
        );
      }

      if (!row.context && row.readers !== false) {
        const query = {
          path: 'evidence.txt',
          classification: {
            predicates: ['Does this contain selected evidence?'],
          },
        };
        const rejected = cli(project, env, [
          'localFetch',
          JSON.stringify({ queries: [query] }),
          '--json',
        ]);
        assert.notEqual(
          rejected.status,
          0,
          `${row.name}: CLI rejects unavailable context`
        );
        assert.match(
          rejected.stdout + rejected.stderr,
          /classification|invalidInput/i
        );
        const mcpRejected = await client.callTool({
          name: 'localFetch',
          arguments: { queries: [query] },
        });
        assert.equal(
          mcpRejected.isError,
          true,
          `${row.name}: MCP rejects unavailable context`
        );
      }
      results.push({
        name: row.name,
        context: row.context,
        clasify: row.clasify,
        passed: true,
      });
    } catch (error) {
      throw new Error(`${row.name}: ${error.message}\n${stderr}`, {
        cause: error,
      });
    } finally {
      await client.close();
    }
  }
} finally {
  await fs.writeFile(
    path.join(base, 'results.json'),
    `${JSON.stringify(results, null, 2)}\n`
  );
  await fs.rm(fixture, { recursive: true, force: true });
}

console.log(`PASS ${results.length} key-based CLI/MCP availability scenarios`);
