// Shared capture-test helpers: an MCP client over the package bin in a temporary
// workspace, capture shaping from literal stdout/stderr and continuation calls.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { captureResult } from '../../dist/engine/capture-result.mjs';
import { connectStdio } from '../../dist/runtime.js';
export const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
export async function fixture(fn) {
  const work = mkdtempSync(join(tmpdir(), 'octo-capture-'));
  let client;
  try {
    client = await connectStdio({
      command: process.execPath,
      args: [bin],
      cwd: work,
    });
    await fn(work, client);
  } finally {
    await client?.close();
    rmSync(work, { recursive: true, force: true });
  }
}
/** Writes each named log under directory and returns the capture outputs map. */
export function writeOutputs(directory, logs) {
  const outputs = {};
  for (const [name, value] of Object.entries(logs)) {
    const file = join(directory, name + '.txt');
    writeFileSync(file, value);
    outputs[name] = { file, bytes: Buffer.byteLength(value) };
  }
  return outputs;
}
export async function shape(work, text, error = '', command = 'run') {
  const directory = join(work, 'capture');
  mkdirSync(directory);
  return captureResult(
    { exitCode: error ? 1 : 0 },
    writeOutputs(directory, { stdout: text, stderr: error }),
    directory,
    command,
    undefined,
    work
  );
}
export const call = (client, next) =>
  client.callTool({ name: next.tool, arguments: next.query }).then(r => {
    assert(!r.isError, r.content?.[0]?.text);
    return r.structuredContent;
  });
/** Runs the package typed CLI (`/cli <args>`) in work. */
export const typedCli = (work, args, options = {}) =>
  spawnSync(process.execPath, [bin, '/cli', ...args], {
    cwd: work,
    encoding: 'utf8',
    ...options,
  });
