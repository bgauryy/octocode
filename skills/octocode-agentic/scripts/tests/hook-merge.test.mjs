import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const script = new URL('../hook-merge.mjs', import.meta.url).pathname;
const run = (args, input) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', input });
const handler = (command) => ({ type: 'command', command, timeout: 10 });
const nested = (command) => ({ hooks: { SessionStart: [{ hooks: [handler(command)] }], PostToolUse: [{ hooks: [handler(command)] }] } });

test('merge keeps unrelated settings and handlers, and a rerun changes nothing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'hook-merge-'));
  try {
    const file = join(dir, 'settings.local.json');
    const existing = { permissions: { allow: ['Bash(ls)'] }, hooks: { PostToolUse: [{ matcher: 'Edit', hooks: [handler('lint')] }] } };
    writeFileSync(file, `${JSON.stringify(existing, null, 2)}\n`);
    const add = join(dir, 'add.json');
    writeFileSync(add, JSON.stringify(nested('comm host-hook --db new')));
    const first = run(['--file', file, '--add', add]);
    assert.equal(first.status, 0, first.stdout);
    assert.deepEqual(JSON.parse(first.stdout), { ok: true, file, written: true, dryRun: false, added: 2, skipped: 0, removed: 0, events: ['SessionStart', 'PostToolUse'] });
    const merged = JSON.parse(readFileSync(file, 'utf8'));
    assert.deepEqual(merged.permissions, existing.permissions);
    assert.deepEqual(merged.hooks.PostToolUse[0], existing.hooks.PostToolUse[0]);
    assert.equal(merged.hooks.PostToolUse.length, 2);
    const before = readFileSync(file, 'utf8');
    const again = JSON.parse(run(['--file', file, '--add', add]).stdout);
    assert.equal(again.written, false);
    assert.equal(again.skipped, 2);
    assert.equal(readFileSync(file, 'utf8'), before);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('--replace drops only matching obsolete handlers and --dry-run writes nothing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'hook-merge-'));
  try {
    const file = join(dir, 'hooks.json');
    const old = nested('comm host-hook --db old');
    old.hooks.Stop = [{ hooks: [handler('comm completion-check'), handler('notify')] }];
    writeFileSync(file, JSON.stringify(old));
    const preview = run(['--file', file, '--add', '-', '--replace', '^comm ', '--dry-run'], JSON.stringify(nested('comm host-hook --db new')));
    assert.equal(preview.status, 0, preview.stdout);
    const body = JSON.parse(preview.stdout);
    assert.equal(body.written, false);
    assert.equal(body.removed, 3);
    assert.deepEqual(body.settings.hooks.Stop, [{ hooks: [handler('notify')] }]);
    assert.deepEqual(body.settings.hooks.SessionStart, nested('comm host-hook --db new').hooks.SessionStart);
    assert.deepEqual(JSON.parse(readFileSync(file, 'utf8')), old);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('flat event arrays keep their version; conflicts and bad input leave the file alone', () => {
  const dir = mkdtempSync(join(tmpdir(), 'hook-merge-'));
  try {
    const file = join(dir, '.cursor', 'hooks.json');
    const add = join(dir, 'add.json');
    writeFileSync(add, JSON.stringify({ version: 1, hooks: { sessionStart: [handler('comm')] } }));
    const created = run(['--file', file, '--add', add]);
    assert.equal(created.status, 0, created.stdout);
    assert.deepEqual(JSON.parse(readFileSync(file, 'utf8')), { version: 1, hooks: { sessionStart: [handler('comm')] } });
    writeFileSync(add, JSON.stringify({ version: 2, hooks: { sessionStart: [handler('comm')] } }));
    const conflict = run(['--file', file, '--add', add]);
    assert.equal(conflict.status, 1);
    assert.equal(JSON.parse(conflict.stdout).error.code, 'versionConflict');
    writeFileSync(add, JSON.stringify({ hooks: { sessionStart: 'not an array' } }));
    assert.equal(run(['--file', file, '--add', add]).status, 2);
    assert.equal(run(['--file', join(dir, 'other.json'), '--add', add, '--replace', '(']).status, 2);
    assert.equal(existsSync(join(dir, 'other.json')), false);
    assert.deepEqual(JSON.parse(readFileSync(file, 'utf8')).version, 1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
