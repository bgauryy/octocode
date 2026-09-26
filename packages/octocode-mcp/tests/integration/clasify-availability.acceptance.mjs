/** Real CLI/MCP credential gates; fake credentials, isolated homes, no provider calls. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const root = process.cwd();
const base = path.join(root, '.octocode/octocode-dev/clasify-availability');
await fs.mkdir(base, { recursive: true });
const fixture = await fs.mkdtemp(path.join(base, 'fixture-'));
const request = {
  reasoning: 'Check disabled capability without provider access.',
  resources: [{ context: { value: 'fixture' } }],
  questions: [{ type: 'noul', instructions: 'Is this a fixture?' }],
};
const cases = [
  {
    name: 'workspace-blank-falls-back',
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    projectEnv: 'OCTOCODE_CLASSIFICATION_API=   \n',
    enabled: true,
  },
  {
    name: 'workspace-disables-home-key',
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    projectEnv: 'DISABLE_TOOLS=clasify\n',
    enabled: false,
  },
  {
    name: 'home-local-off',
    homeEnv: 'ENABLE_LOCAL=false\n',
    enabled: false,
    localEnabled: false,
  },
  {
    name: 'workspace-local-overrides-home',
    homeEnv: 'ENABLE_LOCAL=false\n',
    projectEnv: 'ENABLE_LOCAL=true\n',
    enabled: false,
    localEnabled: true,
  },
  {
    name: 'process-local-overrides-workspace',
    env: { ENABLE_LOCAL: 'false' },
    homeEnv: 'ENABLE_LOCAL=false\n',
    projectEnv: 'ENABLE_LOCAL=true\n',
    enabled: false,
    localEnabled: false,
  },

  { name: 'absent', enabled: false },
  {
    name: 'allowlist-without-key',
    config: { tools: { enabled: ['clasify', 'localFetch'] } },
    enabled: false,
  },
  {
    name: 'disabled-with-key',
    env: { OCTOCODE_CLASSIFICATION_API: 'fixture-key' },
    config: { tools: { disabled: ['clasify'] } },
    enabled: false,
  },
  { name: 'empty', env: { OCTOCODE_CLASSIFICATION_API: '' }, enabled: false },
  {
    name: 'whitespace',
    env: { OCTOCODE_CLASSIFICATION_API: '   ' },
    enabled: false,
  },
  {
    name: 'process-key',
    env: { OCTOCODE_CLASSIFICATION_API: 'fixture-key' },
    enabled: true,
  },
  {
    name: 'home-env-key',
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-key\n',
    enabled: true,
  },
  {
    name: 'home-env-empty',
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=\n',
    enabled: false,
  },
  {
    name: 'home-alias',
    homeEnv: 'OCTOCODE_JEV_KEY=fixture-key\n',
    enabled: true,
  },
  {
    name: 'project-key-only',
    projectEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-key\n',
    enabled: true,
  },
  {
    name: 'config-key',
    config: { classification: { api: 'fixture-key' } },
    enabled: true,
  },
  {
    name: 'empty-overrides-fallbacks',
    env: { OCTOCODE_CLASSIFICATION_API: '', OCTOCODE_JEV_KEY: 'fixture-alias' },
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    config: { classification: { api: 'fixture-config' } },
    enabled: false,
  },
  {
    name: 'whitespace-overrides-fallbacks',
    env: {
      OCTOCODE_CLASSIFICATION_API: '  ',
      OCTOCODE_JEV_KEY: 'fixture-alias',
    },
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    config: { classification: { api: 'fixture-config' } },
    enabled: false,
  },
];
const results = [];
try {
  for (const row of cases) {
    const home = path.join(fixture, row.name, 'home');
    const project = path.join(fixture, row.name, 'project');
    await fs.mkdir(home, { recursive: true });
    await fs.mkdir(path.join(project, '.octocode'), { recursive: true });
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
      ...row.env,
    };
    const client = new Client({ name: 'clasify-availability', version: '1' });
    try {
      await client.connect(
        new StdioClientTransport({
          command: process.execPath,
          args: [path.join(root, 'packages/octocode-mcp/dist/index.js')],
          cwd: project,
          env,
          stderr: 'pipe',
        })
      );
      const catalog = await client.listTools();
      if (row.localEnabled !== undefined)
        assert.equal(
          catalog.tools.some(t => t.name === 'localFetch'),
          row.localEnabled,
          row.name
        );
      const scheme = spawnSync(
        process.execPath,
        [
          path.join(root, 'packages/octocode/out/octocode.js'),
          'scheme',
          'clasify',
          '--compact',
        ],
        { env, cwd: project, encoding: 'utf8', timeout: 15000 }
      );
      assert.equal(scheme.status, 0, row.name);
      assert.equal(
        JSON.parse(scheme.stdout).availability.enabled,
        row.enabled,
        `${row.name}: CLI discovery`
      );
      assert.equal(
        catalog.tools.some(t => t.name === 'clasify'),
        row.enabled,
        row.name
      );
      if (!row.enabled) {
        assert.ok(
          !/\bclasify\b/i.test(client.getInstructions() ?? ''),
          `${row.name}: disabled tool in instructions`
        );
        assert.ok(
          catalog.tools.every(t => !/\bclasify\b/i.test(t.description ?? '')),
          `${row.name}: disabled tool in descriptions`
        );
        await assert.rejects(
          client.callTool({ name: 'clasify', arguments: request }),
          /Tool clasify not found/,
          `${row.name}: direct MCP call must fail`
        );
        const cli = spawnSync(
          process.execPath,
          [
            path.join(root, 'packages/octocode/out/octocode.js'),
            'clasify',
            JSON.stringify(request),
          ],
          { env, cwd: project, encoding: 'utf8', timeout: 15000 }
        );
        assert.notEqual(
          cli.status,
          0,
          `${row.name}: direct CLI call must fail`
        );
        assert.match(
          cli.stdout + cli.stderr,
          /OCTOCODE_CLASSIFICATION_API|classification.*(disabled|unavailable)|not available|disabled/i,
          `${row.name}: actionable CLI error`
        );
      }
      results.push({ name: row.name, enabled: row.enabled, passed: true });
    } finally {
      await client.close();
    }
  }
} finally {
  await fs.writeFile(
    path.join(base, 'results.json'),
    JSON.stringify(results, null, 2) + '\n'
  );
  await fs.rm(fixture, { recursive: true, force: true });
}
console.log(
  `PASS ${results.length}/${cases.length} isolated CLI/MCP credential scenarios`
);
