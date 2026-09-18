import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { resolveRfcPath } from '../src/tools/planning/plan-rfc.js';

function makeRfcWorkspace(): { ws: string; rfcDir: string; rfcFile: string } {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'plan-rfc-ws-'));
  const rfcDir = path.join(ws, '.octocode', 'rfc', 'unify-plan-rfc');
  fs.mkdirSync(rfcDir, { recursive: true });
  const rfcFile = path.join(rfcDir, 'RFC.md');
  fs.writeFileSync(rfcFile, '# RFC\n');
  return { ws, rfcDir, rfcFile };
}

test('resolveRfcPath resolves a directory, file, and workspace-relative path', () => {
  const { ws, rfcDir, rfcFile } = makeRfcWorkspace();
  try {
    assert.equal(resolveRfcPath(ws, rfcDir).path, fs.realpathSync(rfcFile));
    assert.equal(resolveRfcPath(ws, rfcFile).path, fs.realpathSync(rfcFile));
    assert.equal(resolveRfcPath(ws, path.join('.octocode', 'rfc', 'unify-plan-rfc')).path, fs.realpathSync(rfcFile));
  } finally {
    fs.rmSync(ws, { recursive: true, force: true });
  }
});

test('resolveRfcPath enforces containment while allowing workspace forward references', () => {
  const { ws } = makeRfcWorkspace();
  const notes = path.join(ws, 'NOTES.md');
  fs.writeFileSync(notes, '# notes');
  const emptyDir = path.join(ws, '.octocode', 'rfc', 'empty-dir');
  fs.mkdirSync(emptyDir, { recursive: true });
  try {
    assert.equal(resolveRfcPath(ws, notes).error, undefined);
    assert.equal(resolveRfcPath(ws, path.join('.octocode', 'rfc', '..', '..', 'NOTES.md')).error, undefined);
    const forwardDir = resolveRfcPath(ws, path.join('.octocode', 'rfc', 'future'));
    assert.equal(forwardDir.error, undefined);
    assert.ok(forwardDir.path?.endsWith('RFC.md'));
    const forwardFile = resolveRfcPath(ws, path.join('.octocode', 'rfc', 'future', 'RFC.md'));
    assert.equal(forwardFile.error, undefined);
    assert.ok(forwardFile.path?.endsWith('RFC.md'));
    assert.ok(resolveRfcPath(ws, '').error);
    assert.match(resolveRfcPath(ws, emptyDir).error!, /no .md file/);

    const outside = path.join(path.dirname(ws), 'SECRETS.md');
    fs.writeFileSync(outside, '# outside');
    try {
      assert.match(resolveRfcPath(ws, outside).error!, /within the workspace/);
    } finally {
      fs.unlinkSync(outside);
    }
  } finally {
    fs.rmSync(ws, { recursive: true, force: true });
  }
});

test('resolveRfcPath uses the sole markdown file in an existing directory', () => {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'plan-rfc-alt-md-'));
  const rfcDir = path.join(ws, '.octocode', 'rfc', 'design');
  fs.mkdirSync(rfcDir, { recursive: true });
  const design = path.join(rfcDir, 'DESIGN.md');
  fs.writeFileSync(design, '# Design doc\n');
  try {
    const result = resolveRfcPath(ws, rfcDir);
    assert.equal(result.error, undefined);
    assert.equal(result.path, fs.realpathSync(design));
  } finally {
    fs.rmSync(ws, { recursive: true, force: true });
  }
});
