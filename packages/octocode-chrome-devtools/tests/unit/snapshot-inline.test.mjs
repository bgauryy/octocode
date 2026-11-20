import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const { captureResult, INLINE_SNAPSHOT_BYTES } = await import(
  join(import.meta.dirname, '../../dist/engine/capture-result.mjs')
);

async function reply(stdoutText) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-snap-'));
  try {
    const out = join(dir, 'stdout.txt'),
      err = join(dir, 'stderr.txt');
    writeFileSync(out, stdoutText);
    writeFileSync(err, '');
    return await captureResult(
      { exitCode: 0 },
      {
        stdout: { file: out, bytes: Buffer.byteLength(stdoutText) },
        stderr: { file: err, bytes: 0 },
      },
      dir,
      'snapshot',
      undefined,
      dir
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
const artifact = '[ARTIFACT] PAGE_NODES /tmp/page-nodes.txt\n';

test('a complete small snapshot page is inlined with its refs', async () => {
  const page =
    '[PAGE] "T" http://x/\n[METRIC] SNAPSHOT refs=1 showing=e1-e1 page=1/1 totalAxNodes=3\n[SNAPSHOT] [e1] button "Go"\n';
  const result = await reply(page + artifact);
  assert.match(result.snapshot, /\[e1\] button "Go"/);
});

test('a paged snapshot keeps its continuation instead of inlining a partial page', async () => {
  const page =
    '[METRIC] SNAPSHOT refs=900 showing=e1-e100 page=1/9 totalAxNodes=3000\n[SNAPSHOT] [e1] button "Go"\n';
  const result = await reply(page + artifact);
  assert.equal(result.snapshot, undefined);
  assert.ok(result.next.capture);
});

test('a snapshot page over the inline bound is not inlined', async () => {
  const rows = Array.from(
    { length: Math.ceil(INLINE_SNAPSHOT_BYTES / 20) + 1 },
    (_, i) => `[SNAPSHOT] [e${i}] button "B${i}"\n`
  ).join('');
  const result = await reply(
    `[METRIC] SNAPSHOT refs=9 showing=e1-e9 page=1/1 totalAxNodes=9\n${rows}${artifact}`
  );
  assert.equal(result.snapshot, undefined);
});

test('short artifact-free output is returned whole, long output stays behind next.stdout', async () => {
  const { INLINE_STDOUT_BYTES } = await import(
    join(import.meta.dirname, '../../dist/engine/capture-result.mjs')
  );
  const short = await reply('not json: ["sid"]\n');
  assert.equal(short.stdout, 'not json: ["sid"]\n');
  assert.equal(short.next.stdout, undefined);
  const long = await reply('x'.repeat(INLINE_STDOUT_BYTES + 1));
  assert.equal(long.stdout, undefined);
  assert.ok(long.next.stdout);
});
