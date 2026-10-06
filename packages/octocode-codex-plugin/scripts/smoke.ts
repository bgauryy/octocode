import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, globSync } from 'node:fs';
import { cp, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { packageRoot, readJson, repoRoot } from './build.ts';

const run = promisify(execFile);
const temp = await mkdtemp(join(tmpdir(), 'octocode-plugin-smoke-'));
const home = join(temp, 'home');
const codexHome = join(temp, 'codex');
const marketplace = join(temp, 'marketplace');
const fixtureBin = join(temp, 'bin');
const env = {
  PATH: `${fixtureBin}:${process.env.PATH ?? ''}`,
  HOME: home,
  CODEX_HOME: codexHome,
  OCTOCODE_HOME: join(home, '.octocode'),
  GH_CONFIG_DIR: join(home, '.config/gh'),
  OCTOCODE_STORAGE_MODE: 'ephemeral',
  OCTOCODE_ENABLE_LOCAL: 'true',
};
const goal = 'Verify installed plugin behavior against isolated fixtures';
const query = (extra: Record<string, unknown>) => ({
  queries: [{ goal, reasoning: goal, ...extra }],
});
const token = 'octocode-plugin-synthetic-fixture';
let authenticatedRequests = 0;
const server = createServer((request, response) => {
  const authenticated =
    request.headers.authorization === `Bearer ${token}` ||
    request.headers.authorization === `token ${token}`;
  if (authenticated) authenticatedRequests++;
  response.setHeader('Content-Type', 'application/json');
  if (!authenticated) {
    response.writeHead(401);
    response.end(JSON.stringify({ message: 'Bad credentials' }));
    return;
  }
  if (request.url?.includes('/contents/README.md')) {
    response.end(
      JSON.stringify({
        type: 'file',
        name: 'README.md',
        path: 'README.md',
        sha: 'a'.repeat(40),
        size: 22,
        encoding: 'base64',
        content: Buffer.from('private-fixture-content').toString('base64'),
        html_url: 'https://github.com/fixture/private/blob/main/README.md',
      })
    );
  } else if (request.url === '/api/v3/user') {
    response.end(JSON.stringify({ login: 'fixture-user', id: 1 }));
  } else {
    response.writeHead(404);
    response.end(JSON.stringify({ message: 'Not found' }));
  }
});

try {
  for (const path of [
    home,
    codexHome,
    fixtureBin,
    join(marketplace, '.agents/plugins'),
  ])
    await mkdir(path, { recursive: true });
  const packed = JSON.parse(
    (
      await run(
        'npm',
        ['pack', '--ignore-scripts', '--json', '--pack-destination', temp],
        { cwd: packageRoot, timeout: 30_000 }
      )
    ).stdout
  )[0];
  await run('tar', ['-xzf', join(temp, packed.filename), '-C', temp]);
  await cp(join(temp, 'package'), join(marketplace, 'octocode'), {
    recursive: true,
  });
  const catalog = readJson(join(repoRoot, '.agents/plugins/marketplace.json'));
  catalog.name = 'octocode-smoke';
  catalog.plugins[0].source = { source: 'local', path: './octocode' };
  await writeFile(
    join(marketplace, '.agents/plugins/marketplace.json'),
    JSON.stringify(catalog)
  );
  await run('codex', ['plugin', 'marketplace', 'add', marketplace, '--json'], {
    env,
    cwd: temp,
    timeout: 30_000,
  });
  const installed = await run(
    'codex',
    ['plugin', 'add', 'octocode@octocode-smoke', '--json'],
    { env, cwd: temp, timeout: 30_000 }
  );
  assert.ok(
    installed.stdout.includes('octocode'),
    'Codex must report the installed plugin.'
  );
  const cachedManifests = globSync('plugins/cache/**/plugin.json', {
    cwd: codexHome,
  })
    .map(path => join(codexHome, path))
    .filter(path => readJson(path).name === 'octocode');
  assert.equal(
    cachedManifests.length,
    1,
    'Codex must cache the portable plugin manifest.'
  );
  const cachedRoot = dirname(cachedManifests[0]);
  assert.ok(
    existsSync(join(cachedRoot, 'skills/octocode-get-started/SKILL.md'))
  );
  assert.ok(existsSync(join(cachedRoot, 'skills/octocode-research/SKILL.md')));
  console.log(
    'PASS: Codex installed the extracted plugin through an isolated local marketplace.'
  );

  await writeFile(
    join(fixtureBin, 'gh'),
    `#!${process.execPath}\nif (process.argv.slice(2).join(' ') === 'auth token --hostname 127.0.0.1') process.stdout.write(${JSON.stringify(token)}); else process.exitCode = 1;\n`,
    { mode: 0o700 }
  );
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  const runtimeEnv = {
    ...env,
    GITHUB_API_URL: `http://127.0.0.1:${address.port}/api/v3`,
  };
  const metadata = readJson(join(cachedRoot, 'mcp.json')).mcpServers.octocode;
  assert.deepEqual(metadata.args, [
    '-y',
    `octocode-mcp@${readJson(join(repoRoot, 'packages/octocode-mcp/package.json')).version}`,
  ]);
  // Explicit development substitution: exercise the built runtime before its npm version exists.
  for (const project of ['project-a', 'project-b']) {
    const cwd = join(temp, project);
    await mkdir(cwd);
    await writeFile(join(cwd, 'README.md'), project);
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [join(repoRoot, 'packages/octocode-mcp/dist/index.js')],
      cwd,
      env: runtimeEnv,
      stderr: 'pipe',
    });
    const client = new Client({
      name: 'octocode-plugin-smoke',
      version: '0.1.0',
    });
    let stderr = '';
    try {
      const connecting = client.connect(transport);
      transport.stderr?.on('data', chunk => {
        stderr += chunk.toString();
      });
      await connecting;
      const listed = await client.listTools();
      assert.ok(listed.tools.some(tool => tool.name === 'localFetch'));
      const local = await client.callTool({
        name: 'localFetch',
        arguments: query({ path: 'README.md', fullContent: true }),
      });
      assert.ok(
        JSON.stringify(local).includes(project),
        'Relative reads must use the active project.'
      );
      const outside = await client.callTool({
        name: 'localFetch',
        arguments: query({
          path: join(marketplace, 'octocode/plugin.json'),
          fullContent: true,
        }),
      });
      assert.ok(
        JSON.stringify(outside).includes('pathOutsideAllowedRoots'),
        'Unrelated plugin cache must not become a project root.'
      );
      if (project === 'project-a') {
        const status = JSON.parse(
          (
            await run(
              process.execPath,
              [
                join(repoRoot, 'packages/octocode/out/octocode.js'),
                'auth',
                '--json',
              ],
              { cwd, env: runtimeEnv, timeout: 15_000 }
            )
          ).stdout
        );
        assert.equal(status.tokenSource, 'gh-cli');
        const github = await client.callTool({
          name: 'ghGetFileContent',
          arguments: query({
            owner: 'fixture',
            repo: 'private',
            path: 'README.md',
            branch: 'a'.repeat(40),
            fullContent: true,
          }),
        });
        assert.ok(
          JSON.stringify(github).includes('private-fixture-content'),
          JSON.stringify(github)
        );
        assert.ok(
          authenticatedRequests > 0,
          'The local gh credential must reach the GitHub fixture.'
        );
        assert.ok(
          !JSON.stringify(github).includes(token),
          'Credentials must not appear in tool output.'
        );
        await writeFile(
          join(fixtureBin, 'gh'),
          `#!${process.execPath}\nprocess.exitCode = 1;\n`,
          { mode: 0o700 }
        );
      } else {
        const denied = await client.callTool({
          name: 'ghGetFileContent',
          arguments: query({
            owner: 'fixture',
            repo: 'private',
            path: 'README.md',
            branch: 'a'.repeat(40),
            fullContent: true,
            forceRefresh: true,
          }),
        });
        assert.ok(!JSON.stringify(denied).includes('private-fixture-content'));
        assert.match(
          JSON.stringify(denied),
          /401|unauthorized|authentication|credential/i
        );
      }
    } catch (error) {
      throw new Error(
        `Local runtime smoke failed: ${String(error)}\n${stderr.replaceAll(token, '[REDACTED]')}`
      );
    } finally {
      await client.close();
    }
  }
  assert.ok(
    !existsSync(join(home, '.octocode/credentials.json')),
    'Plugin use must not create an Octocode credential store.'
  );
  console.log(
    'PASS: local runtime reads both project roots, rejects unrelated paths, and uses synthetic gh authentication without returning or storing the token.'
  );
  console.log(
    'Registry-pinned launch and other operating systems still require release validation.'
  );
} finally {
  server.close();
  server.closeAllConnections();
  await rm(temp, { recursive: true, force: true });
}
