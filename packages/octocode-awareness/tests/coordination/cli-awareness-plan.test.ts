import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { tsxCli } from '../helpers/tsx-cli.js';

const SOURCE_BIN = resolve(dirname(fileURLToPath(import.meta.url)), '../../bin/awareness.ts');
const TSX = tsxCli;
const roots: string[] = [];
const source = (args: string[]) => spawnSync(process.execPath, [TSX, SOURCE_BIN, ...args], { encoding: 'utf8' });
const json = (text: string) => JSON.parse(text) as Record<string, unknown>;

afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe('canonical root plan CLI', () => {
  it('creates and lists plans through the source root bin', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'aw-root-plan-'));
    roots.push(workspace);
    const created = source(['plan', 'create', '--workspace', workspace, '--lead-agent-id', 'lead', '--name', 'Cross-host plan', '--objective', 'Ship the verified change.', '--compact']);
    expect(created.status, created.stderr || created.stdout).toBe(0);
    const plan = json(created.stdout);
    expect(plan['plan_id']).toMatch(/^plan_/);
    const listed = source(['plan', 'list', '--workspace', workspace, '--compact']);
    expect(listed.status, listed.stderr || listed.stdout).toBe(0);
    expect(json(listed.stdout)['plans']).toEqual(expect.arrayContaining([expect.objectContaining({ plan_id: plan['plan_id'] })]));
  });

  it('uses root agent registry and signal routes rather than adapter aliases', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'aw-root-signal-'));
    roots.push(workspace);
    const registered = source(['agent', 'register', '--workspace', workspace, '--agent-id', 'alice', '--agent-name', 'Alice', '--compact']);
    expect(registered.status, registered.stderr || registered.stdout).toBe(0);
    const signal = source(['signal', 'publish', '--workspace', workspace, '--agent-id', 'alice', '--to-agent', 'bob', '--kind', 'fyi', '--subject', 'review ready', '--body', 'please inspect', '--compact']);
    expect(signal.status, signal.stderr || signal.stdout).toBe(0);
    expect(json(signal.stdout)['signal_id']).toMatch(/^ntf_/);
  });
});
