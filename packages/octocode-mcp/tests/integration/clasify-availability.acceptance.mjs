/** Key-based clasify and reader-selection availability in built CLI and fresh MCP hosts. */
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';

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

function hasEveryBranch(value, field) {
  if (value?.properties) return Object.hasOwn(value.properties, field);
  const branches = value?.oneOf ?? value?.anyOf;
  return (
    !!branches?.length &&
    branches.every(branch => hasEveryBranch(branch, field))
  );
}

function resolveLocalRef(root, value) {
  const prefix = '#/$defs/';
  return value?.$ref?.startsWith(prefix)
    ? root.$defs?.[value.$ref.slice(prefix.length)]
    : value;
}

function assertSelectionSchema(value, enabled, label, compact = false) {
  assert.equal(
    hasEveryBranch(value, 'clasifyContext'),
    enabled,
    `${label}: clasifyContext`
  );
  assert.equal(
    hasEveryBranch(value, 'selection'),
    enabled && !compact,
    `${label}: selection`
  );
  assert.equal(
    hasEveryBranch(value, 'classifiers'),
    false,
    `${label}: removed classifiers alias`
  );
  assert.equal(
    hasEveryBranch(value, 'ranking'),
    false,
    `${label}: removed ranking alias`
  );
  if (enabled) {
    const branches = value.oneOf ?? value.anyOf ?? [value];
    for (const branch of branches) {
      const context = resolveLocalRef(value, branch.properties?.clasifyContext);
      assert.ok(
        context?.properties?.classifiers,
        `${label}: nested classifiers`
      );
      assert.ok(context?.properties?.longContext, `${label}: longContext`);
      assert.ok(
        !branch.required?.includes('clasifyContext'),
        `${label}: context remains optional`
      );
    }
  }
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
    name: 'removed-ranking-gate-cannot-disable-context',
    env: {
      OCTOCODE_CLASSIFICATION_API: 'fixture-key',
      OCTOCODE_BETA: 'false',
      OCTOCODE_RANKING_ENABLED: 'false',
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
      const mcpInstructions = client.getInstructions() ?? '';
      assert.ok(cliCatalog.instructions.startsWith(`${mcpInstructions}\n`));
      for (const instructions of [cliCatalog.instructions, mcpInstructions]) {
        assert.equal(
          instructions.includes('<clasify>'),
          row.context || row.clasify,
          row.name
        );
        assert.equal(
          instructions.includes('clasifyContext'),
          row.context,
          row.name
        );
        assert.equal(
          instructions.includes('clasify scouts unread'),
          row.clasify,
          row.name
        );
      }

      for (const name of selectionTools) {
        const mcpTool = catalog.tools.find(tool => tool.name === name);
        if (row.readers === false) {
          assert.equal(mcpTool, undefined, `${row.name}: ${name} unavailable`);
          continue;
        }
        assert.ok(mcpTool, `${row.name}: MCP ${name}`);
        assertSelectionSchema(
          mcpTool.inputSchema.properties.queries.items,
          row.context,
          `${row.name}: MCP ${name}`,
          true
        );
        for (const view of ['query', 'full']) {
          const cliTool = schema(project, env, [name, '--view', view]);
          assertSelectionSchema(
            cliTool.querySchema,
            row.context,
            `${row.name}: CLI ${name} ${view}`,
            view === 'query'
          );
          assert.ok(
            !cliTool.moreFields?.some(field =>
              [
                'clasifyContext',
                'selection',
                'classifiers',
                'ranking',
              ].includes(field)
            ),
            `${row.name}: selection controls are exposed or absent, never hidden in moreFields`
          );
        }
      }

      if (!row.context && row.readers !== false) {
        const query = {
          path: 'evidence.txt',
          clasifyContext: {
            classifiers: ['Does this contain selected evidence?'],
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
          /clasifyContext|classification.*unavailable|invalidInput/i
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
