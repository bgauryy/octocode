// The engine reads only inside its workspace root (WORKSPACE_ROOT, else its cwd). A build or refresh of a root outside
// the caller's workspace runs the engine rooted where the build ran, or at the root itself, for that run's calls only.
import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tempDir, fakeCatalog } from './test-tmp.mjs';

const lib = fileURLToPath(new URL('../src/runtime/lib.ts', import.meta.url));

test('engine calls inside withEngineRoot run rooted there; outside it, they keep the caller workspace', () => {
  const base = realpathSync(tempDir('graph-engine-root-'));
  const [caller, project, inner] = ['caller', 'project', 'caller/inner'].map(
    d => join(base, d)
  );
  for (const d of [caller, project, inner]) mkdirSync(d, { recursive: true });
  const cli = join(base, 'fake-cli.mjs');
  writeFileSync(
    cli,
    `#!/usr/bin/env node\nif (process.argv[2] === 'whereami') { console.log(JSON.stringify({ cwd: process.cwd(), root: process.env.WORKSPACE_ROOT ?? null })); process.exit(0); }\n${fakeCatalog}\n`
  );
  const script = `
    const { octocode, nativeWorkspaceRoot, withEngineRoot, engineRootFor } = await import(${JSON.stringify(lib)});
    const out = {
      plain: await octocode(['whereami']),
      plainRoot: await nativeWorkspaceRoot(),
      rooted: await withEngineRoot(${JSON.stringify(project)}, () => octocode(['whereami'])),
      rootedRoot: await withEngineRoot(${JSON.stringify(project)}, () => nativeWorkspaceRoot()),
      outside: await engineRootFor(${JSON.stringify(project)}),
      inside: await engineRootFor(${JSON.stringify(inner)}),
      recorded: await engineRootFor(${JSON.stringify(join(project, 'pkg'))}, ${JSON.stringify(project)}),
    };
    console.log(JSON.stringify(out));`;
  const r = spawnSync(process.execPath, ['--input-type=module', '-e', script], {
    cwd: caller,
    encoding: 'utf8',
    env: {
      ...process.env,
      OCTOCODE_CLI: `${process.execPath} ${cli}`,
      WORKSPACE_ROOT: '',
    },
  });
  assert.equal(r.status, 0, r.stderr);
  const out = JSON.parse(r.stdout.trim().split('\n').pop());
  assert.equal(out.plain.cwd, caller);
  assert.equal(out.plainRoot, caller);
  assert.deepEqual(out.rooted, { cwd: project, root: project });
  assert.equal(out.rootedRoot, project);
  assert.equal(
    out.outside,
    project,
    'a root outside the caller workspace roots the engine at itself'
  );
  assert.equal(
    out.inside,
    null,
    'a root inside the caller workspace changes nothing'
  );
  assert.equal(
    out.recorded,
    project,
    'the workspace the build recorded wins when it holds the root'
  );
});
