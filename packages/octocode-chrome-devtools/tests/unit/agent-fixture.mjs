// Copies the package into a temporary skill folder and connects an MCP client to its bin,
// so agent tests exercise the standalone layout without touching this checkout.
import { mkdtempSync, mkdirSync, cpSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { connectStdio } from '../../dist/runtime.js';
import { copyPackage } from '../copy-package.mjs';

export async function fixture(fn) {
  const work = mkdtempSync(join(tmpdir(), 'octo-agent-'));
  const skill = join(work, 'skill');
  copyPackage(resolve(import.meta.dirname, '../..'), skill);
  const cwd = join(work, 'workspace');
  cpSync(join(skill, 'package.json'), join(work, 'package.json'));
  mkdirSync(cwd);
  const client = await connectStdio({
    command: process.execPath,
    args: [join(skill, 'bin/octocode-chrome-devtools.mjs')],
    cwd,
  });
  try {
    await fn({ client, cwd, skill, work });
  } finally {
    await client.close();
    rmSync(work, { recursive: true, force: true });
  }
}
export const data = result =>
  result.structuredContent ??
  JSON.parse(result.content.find(row => row.type === 'text').text);
/** Runs the copied skill's typed CLI (`/cli <args>`). */
export const typedCli = (skill, args, options) =>
  spawnSync(
    process.execPath,
    [join(skill, 'bin/octocode-chrome-devtools.mjs'), '/cli', ...args],
    { encoding: 'utf8', ...options }
  );
