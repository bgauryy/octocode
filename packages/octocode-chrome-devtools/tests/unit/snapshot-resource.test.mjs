import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  readTargetSnapshot,
  recordTargetSnapshot,
} from '../../dist/engine/cdp-checks/snapshot-resource.mjs';

test('switching tabs retains each snapshot and rejects mismatched artifact identity', () => {
  const dir = mkdtempSync(join(tmpdir(), 'octo-snapshot-targets-'));
  const resourcesFile = join(dir, 'resources.json');
  const resources = {};
  const session = id => ({
    targetInfo: { id },
    resourcesFile,
    upsertResourceMap(key, details) {
      resources[key] = details;
      writeFileSync(resourcesFile, JSON.stringify({ resources }));
    },
  });
  try {
    for (const id of ['page-a', 'page-b']) {
      const artifactPath = join(dir, id + '.json');
      writeFileSync(
        artifactPath,
        JSON.stringify({ targetId: id, refs: { e1: { name: id } } })
      );
      recordTargetSnapshot(session(id), { artifactPath });
    }
    for (const id of ['page-a', 'page-b', 'page-a'])
      assert.equal(readTargetSnapshot(session(id)).data.refs.e1.name, id);
    assert.equal(readTargetSnapshot(session('unseen')), null);
    writeFileSync(
      join(dir, 'page-a.json'),
      readFileSync(join(dir, 'page-b.json'))
    );
    assert.equal(readTargetSnapshot(session('page-a')), null);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
