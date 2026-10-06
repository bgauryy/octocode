/** Run from repo root after config/native/CLI/MCP builds. Fake keys, loopback only. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
const exec = promisify(execFile);
const root = process.cwd();
const base = path.join(root, '.octocode/octocode-dev/token-precedence');
await fs.mkdir(base, { recursive: true });
const fixture = await fs.mkdtemp(path.join(base, 'fixture-'));
const observations = [];
const server = http.createServer((req, res) => {
  observations.push(req.headers.authorization);
  req.resume();
  res.writeHead(401, { 'Content-Type': 'application/json' });
  res.end('{"message":"synthetic credential probe"}');
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const endpoint = `http://127.0.0.1:${server.address().port}`;
const groups = [
  {
    tool: 'ghSearchRepo',
    aliases: ['GH_TOKEN', 'GITHUB_TOKEN'],
    input: {
      queries: [
        {
          reasoning: 'Probe source precedence on a local fixture endpoint.',
          keywords: ['fixture'],
        },
      ],
    },
  },
  {
    tool: 'clasify',
    aliases: ['OCTOCODE_CLASSIFICATION_API'],
    input: {
      reasoning: 'Probe source precedence on a local fixture endpoint.',
      resources: [{ context: { value: 'fixture' } }],
      questions: [{ type: 'noul', instructions: 'Is this a fixture?' }],
    },
  },
];
const results = [];
try {
  for (const group of groups)
    for (const alias of group.aliases)
      for (const source of ['workspace', 'process']) {
        const name = `${group.tool}-${alias}-${source}`;
        const home = path.join(fixture, name, 'home'),
          cwd = path.join(fixture, name, 'workspace');
        await fs.mkdir(home, { recursive: true });
        await fs.mkdir(path.join(cwd, '.octocode'), { recursive: true });
        await fs.writeFile(
          path.join(home, '.env'),
          `${group.aliases[0]}=home-fixture\n`
        );
        await fs.writeFile(
          path.join(cwd, '.octocode/.env'),
          `${source === 'workspace' ? alias : group.aliases[0]}=workspace-fixture\n`
        );
        await fs.writeFile(
          path.join(home, '.octocoderc'),
          '{"classification":{"api":"config-fixture"}}'
        );
        const env = {
          PATH: process.env.PATH ?? '',
          HOME: path.dirname(home),
          OCTOCODE_HOME: home,
          GITHUB_API_URL: endpoint,
          OCTOCODE_CLASSIFICATION_API_HOST: endpoint,
          ...(source === 'process' ? { [alias]: 'process-fixture' } : {}),
        };
        for (const surface of ['cli', 'mcp']) {
          observations.length = 0;
          if (surface === 'cli') {
            try {
              await exec(
                process.execPath,
                [
                  path.join(root, 'packages/octocode/out/octocode.js'),
                  group.tool,
                  JSON.stringify(group.input),
                ],
                { cwd, env, timeout: 15000 }
              );
            } catch (error) {
              if (error.killed) throw error;
            }
          } else {
            const client = new Client({
              name: 'token-precedence',
              version: '1',
            });
            try {
              await client.connect(
                new StdioClientTransport({
                  command: process.execPath,
                  args: [
                    path.join(root, 'packages/octocode-mcp/dist/index.js'),
                  ],
                  cwd,
                  env,
                  stderr: 'pipe',
                })
              );
              await client.callTool({
                name: group.tool,
                arguments: group.input,
              });
            } finally {
              await client.close();
            }
          }
          assert.ok(
            observations.length > 0,
            `${name}/${surface}: no request reached local provider`
          );
          assert.ok(
            observations.every(
              value =>
                value === `Bearer ${source}-fixture` ||
                value === `token ${source}-fixture`
            ),
            `${name}/${surface}: wrong source selected`
          );
          results.push({ name, surface, passed: true });
        }
      }
} finally {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await fs.rm(fixture, { recursive: true, force: true });
  await fs.writeFile(
    path.join(base, 'results.json'),
    JSON.stringify(results, null, 2) + '\n'
  );
}
console.log(
  `PASS ${results.length} real CLI/MCP credential-selection probes; only fake keys sent to loopback`
);
