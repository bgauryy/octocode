import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('shutdown bounds an engine that ignores SIGTERM and preserves its captured evidence', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'chrome-shutdown-'));
  const pidFile = join(directory, 'engine.pid');
  try {
    await build({
      entryPoints: [
        new URL('../../src/invocation.ts', import.meta.url).pathname,
      ],
      outfile: join(directory, 'invocation.mjs'),
      bundle: true,
      platform: 'node',
      format: 'esm',
      target: 'node24',
    });
    mkdirSync(join(directory, 'engine'));
    writeFileSync(
      join(directory, 'engine/cli.mjs'),
      `import {writeFileSync} from 'node:fs';process.on('SIGTERM',()=>{});writeFileSync(${JSON.stringify(pidFile)},String(process.pid));console.log('shutdown evidence');setInterval(()=>{},1000);`
    );
    writeFileSync(
      join(directory, 'fixture.mjs'),
      `import {invoke,stopEngine} from './invocation.mjs';import {existsSync,readFileSync} from 'node:fs';import assert from 'node:assert/strict';const result=invoke('schema',{}).catch(error=>error);while(!existsSync(${JSON.stringify(pidFile)}))await new Promise(resolve=>setTimeout(resolve,10));stopEngine();stopEngine();const error=await result;const capture=JSON.parse(error.message);assert.equal(capture.ok,false);assert.equal(capture.signal,'SIGKILL');assert.match(readFileSync(capture.logs.stdout,'utf8'),/shutdown evidence/);console.log('bounded shutdown verified');`
    );
    const result = await promisify(execFile)(
      process.execPath,
      [join(directory, 'fixture.mjs')],
      {
        cwd: directory,
        timeout: 8500,
        killSignal: 'SIGKILL',
      }
    );
    assert.match(result.stdout, /bounded shutdown verified/);
  } finally {
    if (existsSync(pidFile)) {
      try {
        process.kill(Number(readFileSync(pidFile, 'utf8')), 'SIGKILL');
      } catch (error) {
        if (error.code !== 'ESRCH') throw error;
      }
    }
    rmSync(directory, { recursive: true, force: true });
  }
});

test('cancellation stops an active engine, retains evidence, and never launches a cancelled queued request', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'chrome-cancel-'));
  try {
    await build({
      entryPoints: [
        new URL('../../src/invocation.ts', import.meta.url).pathname,
      ],
      outfile: join(directory, 'invocation.mjs'),
      bundle: true,
      platform: 'node',
      format: 'esm',
      target: 'node24',
    });
    mkdirSync(join(directory, 'engine'));
    writeFileSync(
      join(directory, 'engine/cli.mjs'),
      `import {writeFileSync,appendFileSync} from 'node:fs';appendFileSync('launches',process.argv[2]+'\\n');console.log('partial evidence');setTimeout(()=>writeFileSync('ready','yes'),20);setInterval(()=>{},1000);`
    );
    writeFileSync(
      join(directory, 'fixture.mjs'),
      `import {invoke} from './invocation.mjs';import {existsSync,readFileSync} from 'node:fs';import assert from 'node:assert/strict';const active=new AbortController();const pending=new AbortController();const first=invoke('schema',{},active.signal).catch(e=>e);const second=invoke('targets',{},pending.signal).catch(e=>e);while(!existsSync('ready'))await new Promise(r=>setTimeout(r,10));pending.abort();active.abort();const capture=JSON.parse((await first).message);assert.equal(capture.cancelled,true);assert.equal(capture.outcomeUncertain,true);assert.match(readFileSync(capture.logs.stdout,'utf8'),/partial evidence/);assert.equal((await second).name,'AbortError');assert.equal(readFileSync('launches','utf8'),'schema\\n');console.log('cancellation verified');`
    );
    const result = await promisify(execFile)(
      process.execPath,
      [join(directory, 'fixture.mjs')],
      { cwd: directory, timeout: 8000, killSignal: 'SIGKILL' }
    );
    assert.match(result.stdout, /cancellation verified/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
