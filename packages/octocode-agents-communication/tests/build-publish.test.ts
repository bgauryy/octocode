// The build publishes into dist/ while other processes start from it: every CLI call and hook is a fresh process that imports
// dist/cli.js, dist/mcp.js or dist/hosts/*.js. At every step of the publish, each entry in dist/ must load with all its imports.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { root } from '../src/entry-points.ts';
import { lockText, outputs, publishDist, reclaimLock, stageAndPublish, waitingMessage, type Holder } from '../src/publish-dist.ts';
import { processStart } from '../src/runtime/process-identity.ts';
import { sha256 } from '../src/script-support.ts';

const entries = (dist: string): string[] => outputs(dist).filter(path => !path.startsWith('chunk-')).sort();
const chunks = (dist: string): string[] => outputs(dist).filter(path => path.startsWith('chunk-')).sort();

function write(directory: string, files: Record<string, string>): void {
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(directory, path)), { recursive: true });
    writeFileSync(join(directory, path), text);
  }
}

/** Loads every entry of `dist` in a fresh Node process, as a CLI call or a hook would, and returns their exports. */
function load(dist: string): Record<string, unknown> {
  const script = `const out = {}; for (const [name, url] of ${JSON.stringify(entries(dist).map(path => [path, pathToFileURL(join(dist, path)).href]))}) out[name] = { ...(await import(url)) }; console.log(JSON.stringify(out));`;
  const run = spawnSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  return JSON.parse(run.stdout) as Record<string, unknown>;
}

/** An onStep that loads every entry after the step and names the step that broke one. */
function checked(dist: string, label = ''): (step: string) => void {
  let count = 0;
  return step => {
    count += 1;
    try { load(dist); } catch (error) { assert.fail(`${label}after step ${count} (${step}) an entry in dist/ does not load: ${(error as Error).message}`); }
  };
}

/** The relative files a module imports: static, side-effect, dynamic and re-export forms, minified or not. */
function importedFiles(text: string): string[] {
  return [...text.matchAll(/\b(?:from|import)\s*\(?\s*["'](\.\.?\/[^"']+)["']/g)].map(([, specifier]) => specifier);
}

/** A process id that no process has: a child that already exited. */
function deadPid(): number {
  const pid = spawnSync(process.execPath, ['-e', '']).pid;
  assert.ok(pid);
  return pid;
}

function withBase(run: (dist: string) => Promise<void>): () => Promise<void> {
  return async () => {
    const base = mkdtempSync(join(tmpdir(), 'build-publish-'));
    try { await run(join(base, 'dist')); } finally { rmSync(base, { recursive: true, force: true }); }
  };
}

test('publishing a new build over an old one keeps every entry in dist/ loadable after every step, then prunes unreferenced chunks', withBase(async dist => {
  write(dist, {
    'chunk-AAAA.js': "import { deep } from './chunk-AAA2.js';\nexport const value = `old-${deep}`;\n",
    'chunk-AAA2.js': 'export const deep = 1;\n',
    'chunk-ZZZZ.js': 'export const stale = true;\n',
    'cli.js': "import { value } from './chunk-AAAA.js';\nexport const cli = value;\n",
    'hosts/guard.js': "import { value } from '../chunk-AAAA.js';\nexport const guard = value;\n",
  });
  assert.deepEqual(load(dist), { 'cli.js': { cli: 'old-1' }, 'hosts/guard.js': { guard: 'old-1' } });

  const first = join(dist, '.next-1');
  write(first, {
    'chunk-BBBB.js': "import { leaf } from './chunk-CCCC.js';\nexport const value = `new-${leaf}`;\n",
    'chunk-CCCC.js': 'export const leaf = 2;\n',
    'cli.js': "import { value } from './chunk-BBBB.js';\nexport const cli = value;\n",
    // An entry that imports another entry: the imported one must be in place first.
    'runtime.js': "import { cli } from './cli.js';\nimport './chunk-BBBB.js';\nexport const runtime = cli;\n",
    'hosts/guard.js': "import { leaf } from '../chunk-CCCC.js';\nexport const guard = leaf;\n",
  });
  await publishDist(first, dist, checked(dist));
  assert.deepEqual(load(dist), { 'cli.js': { cli: 'new-2' }, 'hosts/guard.js': { guard: 2 }, 'runtime.js': { runtime: 'new-2' } });
  // The replaced build's chunks stay for processes that loaded it; a chunk neither build references goes.
  assert.deepEqual(chunks(dist), ['chunk-AAA2.js', 'chunk-AAAA.js', 'chunk-BBBB.js', 'chunk-CCCC.js']);
  assert.deepEqual(outputs(first), []);

  const second = join(dist, '.next-2');
  write(second, {
    'chunk-DDDD.js': 'export const value = 3;\n',
    'cli.js': "import { value } from './chunk-DDDD.js';\nexport const cli = value;\n",
    'runtime.js': "import { value } from './chunk-DDDD.js';\nexport const runtime = value;\n",
    'hosts/guard.js': "import { value } from '../chunk-DDDD.js';\nexport const guard = value;\n",
  });
  await publishDist(second, dist, checked(dist));
  assert.deepEqual(load(dist), { 'cli.js': { cli: 3 }, 'hosts/guard.js': { guard: 3 }, 'runtime.js': { runtime: 3 } });
  assert.deepEqual(chunks(dist), ['chunk-BBBB.js', 'chunk-CCCC.js', 'chunk-DDDD.js']);
}));

test('two builds of different sources publishing at once leave every entry loadable: the second waits for the first', withBase(async dist => {
  write(dist, { 'chunk-OOOO.js': 'export const value = 0;\n', 'cli.js': "export { value as cli } from './chunk-OOOO.js';\n" });
  const buildA = async (staging: string) => write(staging, { 'chunk-AAAA.js': "export const value = 'a';\n", 'cli.js': "export { value as cli } from './chunk-AAAA.js';\n" });
  const buildB = async (staging: string) => write(staging, { 'chunk-BBBB.js': "export const value = 'b';\n", 'cli.js': "export { value as cli } from './chunk-BBBB.js';\n" });
  let second: Promise<void> | undefined;
  let secondWaited = false;
  const checkA = checked(dist, 'build A: ');
  // Build B starts while build A is between moving its chunk and moving its entry, and runs as far as it is allowed to.
  await stageAndPublish(dist, buildA, {
    onStep: async step => {
      checkA(step);
      if (second) return;
      let waiting!: () => void;
      const waited = new Promise<void>(resolve => { waiting = resolve; });
      second = stageAndPublish(dist, buildB, { onStep: checked(dist, 'build B: '), onWait: () => { secondWaited = true; waiting(); } });
      await Promise.race([second, waited]);
    },
  });
  await second;
  assert.ok(secondWaited, 'build B waited for build A to finish publishing');
  assert.deepEqual(load(dist), { 'cli.js': { cli: 'b' } });
  assert.deepEqual(readdirSync(dist).filter(name => name.startsWith('.')), []);
}));

test('a build sweeps the staging directories and the lock that interrupted builds left behind', withBase(async dist => {
  write(dist, { '.next-killed/cli.js': 'export const cli = 0;\n', '.next-interrupted/chunk-XXXX.js': '', '.publish.lock': `${deadPid()} interrupted\n` });
  await stageAndPublish(dist, async staging => write(staging, { 'cli.js': 'export const cli = 1;\n' }));
  assert.deepEqual(load(dist), { 'cli.js': { cli: 1 } });
  assert.deepEqual(readdirSync(dist).filter(name => name.startsWith('.')), []);
}));

test('an entry the new build no longer emits is removed with its declaration stub', withBase(async dist => {
  write(dist, {
    'chunk-OLDD.js': 'export const value = 0;\n',
    'cli.js': 'export const cli = 0;\n',
    'hosts/old.js': "export { value as old } from '../chunk-OLDD.js';\n",
    'hosts/old.d.ts': 'export declare const old: number;\n',
  });
  await stageAndPublish(dist, async staging => write(staging, { 'cli.js': 'export const cli = 1;\n' }), { onStep: checked(dist) });
  assert.deepEqual(entries(dist), ['cli.js']);
  assert.equal(existsSync(join(dist, 'hosts/old.d.ts')), false);
  await stageAndPublish(dist, async staging => write(staging, { 'cli.js': 'export const cli = 2;\n' }));
  assert.deepEqual(chunks(dist), []);
}));

test('the import scan sees static, side-effect, dynamic and re-export forms', () => {
  const text = [
    "import './a.js';", 'import { x } from "./b.js";', "export * from './c.js';", "export { y } from '../d.js';",
    'const m = await import("./e.js");', "import('./f.js').then(String);", 'export{z}from"./g.js";import"./h.js";',
  ].join('\n');
  assert.deepEqual(importedFiles(text), ['./a.js', './b.js', './c.js', '../d.js', './e.js', './f.js', './g.js', './h.js']);
});

test('the built dist/ has no staging directory or lock left and every entry finds each file it imports', () => {
  const dist = join(root, 'dist');
  assert.deepEqual(readdirSync(dist).filter(name => name.startsWith('.')), []);
  for (const entry of entries(dist)) {
    const pending = [entry];
    for (const seen = new Set<string>(); pending.length > 0;) {
      const path = pending.pop()!;
      if (seen.has(path)) continue;
      seen.add(path);
      for (const specifier of importedFiles(readFileSync(join(dist, path), 'utf8'))) {
        const target = join(dirname(path), specifier);
        assert.ok(existsSync(join(dist, target)), `${entry}: ${path} imports ${specifier}, which is missing`);
        pending.push(target);
      }
    }
  }
});

test('a build that waits is told which process holds the lock, when it started and since when', withBase(async dist => {
  let holder: Holder | undefined;
  let release!: () => void;
  const waited = new Promise<void>(resolve => { release = resolve; });
  let second: Promise<void> | undefined;
  await stageAndPublish(dist, async staging => {
    second = stageAndPublish(dist, async next => write(next, { 'cli.js': 'export const cli = 2;\n' }), { onWait: found => { holder = found; release(); } });
    await waited;
    write(staging, { 'cli.js': 'export const cli = 1;\n' });
  });
  await second;
  assert.deepEqual(load(dist), { 'cli.js': { cli: 2 } });
  assert.equal(holder?.pid, process.pid);
  assert.equal(holder.start, processStart(process.pid));
  assert.ok(holder.since > 0 && holder.since <= Date.now(), `since ${holder.since}`);
  assert.match(waitingMessage(holder), new RegExp(`pid ${process.pid}\\b`));
}));

test('a lock whose pid now belongs to another process (pid reuse) is taken over, not waited on', { skip: process.platform === 'win32' && 'no process start time on Windows' }, withBase(async dist => {
  mkdirSync(dist, { recursive: true });
  writeFileSync(join(dist, '.publish.lock'), lockText(process.pid, 'an earlier process'));
  await stageAndPublish(dist, async staging => write(staging, { 'cli.js': 'export const cli = 1;\n' }), { onWait: () => { throw Error('waited on a lock whose owner is gone'); } });
  assert.deepEqual(load(dist), { 'cli.js': { cli: 1 } });
  assert.deepEqual(readdirSync(dist).filter(name => name.startsWith('.')), []);
}));

test('reclaiming a dead lock is atomic: a late reclaimer never moves the lock a live build took since', withBase(async dist => {
  mkdirSync(dist, { recursive: true });
  const lock = join(dist, '.publish.lock');
  const dead = lockText(deadPid(), null);
  const live = lockText(process.pid, processStart(process.pid));
  writeFileSync(lock, live);
  const before = statSync(lock, { bigint: true }).ctimeNs;
  reclaimLock(dist, dead);
  assert.equal(readFileSync(lock, 'utf8'), live);
  assert.equal(statSync(lock, { bigint: true }).ctimeNs, before, 'the live lock was renamed or relinked');
  assert.deepEqual(readdirSync(dist), ['.publish.lock']);
  // The dead lock itself goes, also when an earlier reclaimer died holding its marker.
  writeFileSync(lock, dead);
  writeFileSync(join(dist, `.publish-reclaim-${sha256(dead).slice(0, 32)}`), lockText(deadPid(), null));
  reclaimLock(dist, dead);
  assert.deepEqual(readdirSync(dist), []);
}));

test('declarations are published by rename with the build: a stub never points at a missing file, and old ones go', withBase(async dist => {
  const stub = (from: string, name: string) => `export * from '${from}types/pkg/src/${name}.js';\n`;
  write(dist, {
    'cli.js': 'export const cli = 0;\n', 'cli.d.ts': stub('./', 'cli'), 'types/pkg/src/cli.d.ts': 'export declare const cli: 0;\n',
    'hosts/old.js': 'export const old = 0;\n', 'hosts/old.d.ts': stub('../', 'hosts/old'), 'types/pkg/src/hosts/old.d.ts': 'export declare const old: 0;\n',
  });
  const staging = join(dist, '.next-1');
  write(staging, { 'cli.js': 'export const cli = 1;\n', 'cli.d.ts': stub('./', 'cli'), 'types/pkg/src/cli.d.ts': 'export declare const cli: 1;\n' });
  const stubsResolve = (step: string) => {
    for (const path of readdirSync(dist, { recursive: true, encoding: 'utf8' }).filter(name => name.endsWith('.d.ts') && !name.startsWith('types') && !name.startsWith('.'))) {
      for (const specifier of importedFiles(readFileSync(join(dist, path), 'utf8'))) {
        assert.ok(existsSync(join(dist, dirname(path), specifier.replace(/\.js$/, '.d.ts'))), `after ${step}: ${path} points at missing ${specifier}`);
      }
    }
  };
  await publishDist(staging, dist, stubsResolve);
  assert.equal(readFileSync(join(dist, 'types/pkg/src/cli.d.ts'), 'utf8'), 'export declare const cli: 1;\n');
  assert.equal(existsSync(join(dist, 'hosts/old.d.ts')), false);
  assert.equal(existsSync(join(dist, 'types/pkg/src/hosts/old.d.ts')), false);
  assert.deepEqual(readdirSync(staging, { recursive: true, encoding: 'utf8' }).filter(name => name.includes('.')), []);
}));

test('a reclaim marker held by a live process is a holder to wait for, named in the message, not a reason to retry at once', withBase(async dist => {
  mkdirSync(dist, { recursive: true });
  const dead = lockText(deadPid(), null);
  writeFileSync(join(dist, '.publish.lock'), dead);
  const marker = join(dist, `.publish-reclaim-${sha256(dead).slice(0, 32)}`);
  writeFileSync(marker, lockText(process.pid, processStart(process.pid)));
  const holder = reclaimLock(dist, dead);
  assert.equal(holder?.pid, process.pid);
  assert.equal(holder.file, basename(marker));
  assert.equal(readFileSync(join(dist, '.publish.lock'), 'utf8'), dead);
  // A build waits on it as on a live lock, then goes on once the marker's owner is done with it.
  let told: Holder | undefined;
  await stageAndPublish(dist, async staging => write(staging, { 'cli.js': 'export const cli = 1;\n' }), { onWait: found => { told = found; rmSync(marker); } });
  assert.equal(told?.pid, process.pid);
  assert.match(waitingMessage(told), new RegExp(`pid ${process.pid}\\b.*dist/${basename(marker)}`));
  assert.deepEqual(load(dist), { 'cli.js': { cli: 1 } });
  assert.deepEqual(readdirSync(dist).filter(name => name.startsWith('.')), []);
}));

test('`yarn types` (src/emit-types.ts) leaves dist/ alone: only the locked build writes declarations', () => {
  const identity = () => [statSync(join(root, 'dist/types')).ino, statSync(join(root, 'dist/cli.d.ts')).ino, statSync(join(root, 'dist/cli.d.ts')).mtimeMs];
  const before = identity();
  const run = spawnSync(process.execPath, [join(root, 'src/emit-types.ts')], { cwd: root, encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  assert.deepEqual(identity(), before);
});
