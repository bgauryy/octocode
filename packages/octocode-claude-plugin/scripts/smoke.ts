import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { existsSync, globSync, readFileSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { packageRoot, readJson, repoRoot } from './build.ts';

const run = promisify(execFile);
const temp = await mkdtemp(join(tmpdir(), 'octocode-claude-smoke-'));
const home = join(temp, 'home');
const claudeHome = join(temp, 'claude');
const marketplace = join(temp, 'marketplace');
const env = {
  PATH: process.env.PATH ?? '',
  HOME: home,
  CLAUDE_CONFIG_DIR: claudeHome,
  OCTOCODE_HOME: join(home, '.octocode'),
  GH_CONFIG_DIR: join(home, '.config/gh'),
  OCTOCODE_STORAGE_MODE: 'ephemeral',
  ENABLE_LOCAL: 'true',
  npm_config_userconfig: join(home, '.npmrc'),
};
let archive = Buffer.alloc(0);
let registry = '';
let downloads = 0;
const pkg = readJson(join(packageRoot, 'package.json'));
const server = createServer((request, response) => {
  if (request.url === '/plugin.tgz') {
    downloads++;
    response.setHeader('Content-Type', 'application/octet-stream');
    response.end(archive);
    return;
  }
  const path = decodeURIComponent(request.url ?? '');
  if (path !== `/${pkg.name}` && path !== `/${pkg.name}/${pkg.version}`) {
    response.writeHead(404);
    response.end('{}');
    return;
  }
  const version = {
    ...pkg,
    dist: {
      tarball: `${registry}/plugin.tgz`,
      shasum: createHash('sha1').update(archive).digest('hex'),
      integrity: `sha512-${createHash('sha512').update(archive).digest('base64')}`,
    },
  };
  response.setHeader('Content-Type', 'application/json');
  response.end(
    JSON.stringify(
      path.endsWith(`/${pkg.version}`)
        ? version
        : {
            name: pkg.name,
            'dist-tags': { latest: pkg.version },
            versions: { [pkg.version]: version },
          }
    )
  );
});

try {
  for (const path of [home, claudeHome, join(marketplace, '.claude-plugin')])
    await mkdir(path, { recursive: true });
  await writeFile(env.npm_config_userconfig, '');
  const packed = JSON.parse(
    (
      await run(
        'npm',
        ['pack', '--ignore-scripts', '--json', '--pack-destination', temp],
        { cwd: packageRoot, timeout: 30_000 }
      )
    ).stdout
  )[0];
  archive = readFileSync(join(temp, packed.filename));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  registry = `http://127.0.0.1:${address.port}`;
  // Only this empty test profile uses the loopback registry; no saved npm tokens are inherited.
  await writeFile(env.npm_config_userconfig, `registry=${registry}\n`);
  const catalog = readJson(join(repoRoot, '.claude-plugin/marketplace.json'));
  catalog.name = 'octocode-smoke';
  catalog.plugins[0].source.registry = registry;
  await writeFile(
    join(marketplace, '.claude-plugin/marketplace.json'),
    JSON.stringify(catalog)
  );
  const options = {
    env,
    cwd: temp,
    timeout: 60_000,
    maxBuffer: 2 * 1024 * 1024,
  };
  await run('claude', ['plugin', 'marketplace', 'add', marketplace], options);
  await run(
    'claude',
    ['plugin', 'install', 'octocode@octocode-smoke'],
    options
  );
  assert.ok(
    downloads > 0,
    'Claude must fetch the npm archive from the fixture registry.'
  );
  const installed = JSON.parse(
    (await run('claude', ['plugin', 'list', '--json'], options)).stdout
  );
  assert.ok(JSON.stringify(installed).includes('octocode@octocode-smoke'));
  const manifests = globSync('plugins/cache/**/.claude-plugin/plugin.json', {
    cwd: claudeHome,
  });
  assert.equal(manifests.length, 1, 'Claude must cache exactly one plugin.');
  const cachedRoot = dirname(dirname(join(claudeHome, manifests[0])));
  await run('claude', ['plugin', 'validate', cachedRoot, '--strict'], options);
  assert.ok(existsSync(join(cachedRoot, 'skills/octocode-research/SKILL.md')));
  assert.ok(
    existsSync(join(cachedRoot, 'skills/octocode-get-started/SKILL.md'))
  );
  assert.deepEqual(
    readJson(join(cachedRoot, '.mcp.json')),
    readJson(join(packageRoot, '.mcp.json'))
  );
  console.log(
    'PASS: Claude installed and validated the real npm archive from an isolated registry.'
  );

  // The pinned registry runtime is not yet published. Exercise its local build explicitly.
  for (const project of ['project-a', 'project-b']) {
    const cwd = join(temp, project);
    await mkdir(cwd);
    await writeFile(join(cwd, 'README.md'), project);
    const client = new Client({
      name: 'octocode-claude-smoke',
      version: pkg.version,
    });
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [join(repoRoot, 'packages/octocode-mcp/dist/index.js')],
      cwd,
      env,
      stderr: 'pipe',
    });
    try {
      await client.connect(transport);
      const listed = await client.listTools();
      assert.ok(listed.tools.some(tool => tool.name === 'localFetch'));
      const goal = 'Verify the Claude plugin runtime uses the active project';
      const result = await client.callTool({
        name: 'localFetch',
        arguments: {
          queries: [
            { goal, reasoning: goal, path: 'README.md', fullContent: true },
          ],
        },
      });
      assert.ok(
        JSON.stringify(result).includes(project),
        JSON.stringify(result)
      );
      const outside = await client.callTool({
        name: 'localFetch',
        arguments: {
          queries: [
            {
              goal,
              reasoning: goal,
              path: join(cachedRoot, '.mcp.json'),
              fullContent: true,
            },
          ],
        },
      });
      assert.ok(JSON.stringify(outside).includes('pathOutsideAllowedRoots'));
    } finally {
      await client.close();
    }
  }
  console.log(
    'PASS: real local MCP reads both project roots and rejects unrelated plugin cache paths.'
  );
  console.log(
    'Public-registry runtime launch and other operating systems still require release validation.'
  );
} finally {
  server.close();
  server.closeAllConnections();
  await rm(temp, { recursive: true, force: true });
}
