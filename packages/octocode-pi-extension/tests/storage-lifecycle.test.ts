import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { closeOctocodeDb } from '@octocodeai/agent-contracts/db';
import { getMcpEnablement, getSkillEnablement, setMcpServerEnabled, setMcpToolEnabled, setSkillEnabled } from '@octocodeai/agent-contracts/mcp-state';
import { extensionStateDbPath } from '../src/extension-paths.js';
import { openOctocodeDb } from '../src/tools/storage-policy.js';

test('extension enablement survives reopening and isolates workspace overrides', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-storage-lifecycle-'));
  const originalHome = process.env['OCTOCODE_HOME'];
  const originalMode = process.env['OCTOCODE_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = root;
  process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';
  const dbPath = extensionStateDbPath();
  const firstWorkspace = path.join(root, 'first');
  const secondWorkspace = path.join(root, 'second');
  try {
    let db = openOctocodeDb();
    setMcpServerEnabled(db, '*', 'example', false);
    setMcpToolEnabled(db, firstWorkspace, 'example', 'lookup', true);
    setSkillEnabled(db, '*', 'example-skill', false);
    setSkillEnabled(db, firstWorkspace, 'example-skill', true);
    closeOctocodeDb(dbPath);
    db = openOctocodeDb();
    assert.equal(getMcpEnablement(db, firstWorkspace, 'example', 'lookup', true), true);
    assert.equal(getMcpEnablement(db, secondWorkspace, 'example', 'lookup', true), false);
    assert.equal(getMcpEnablement(db, firstWorkspace, 'example', 'another', true), false);
    assert.equal(getSkillEnablement(db, firstWorkspace, 'example-skill'), true);
    assert.equal(getSkillEnablement(db, secondWorkspace, 'example-skill'), false);
    assert.equal((db.prepare('PRAGMA integrity_check').get() as { integrity_check: string }).integrity_check, 'ok');
    assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(), []);
    assert.equal(fs.existsSync(path.join(root, 'awareness')), false);
  } finally {
    closeOctocodeDb(dbPath);
    if (originalHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = originalHome;
    if (originalMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = originalMode;
    fs.rmSync(root, { recursive: true, force: true });
  }
});
