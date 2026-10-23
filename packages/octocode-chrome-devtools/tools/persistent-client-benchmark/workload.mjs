// One arm's workload: complete, filtered and empty reads of the owned fixture extraction,
// then the public page h1. Digests pin identical evidence across every arm and trial.
import assert from 'node:assert/strict';
import { hash } from './integrity.mjs';
import { extraction, queryAll } from './session.mjs';

/** Fixture and public evidence digests, set by the first arm and compared by the rest. */
export const digests = { fixture: null, public: null };

export async function measureArm({ connection, fixture, publicUrl, limit }) {
  const file = await extraction(connection, fixture, 'a');
  const complete = await queryAll({
    file,
    ...(limit === 3 ? { limit } : {}),
  });
  assert.equal(complete.rows.length, 24);
  assert.equal(complete.scanned, 24);
  const digest = hash(JSON.stringify(complete.rows));
  if (digests.fixture === null) digests.fixture = digest;
  assert.equal(digest, digests.fixture);
  const filtered = await queryAll({
    file,
    limit,
    where: [{ path: '/text', op: 'contains', value: 'boundary' }],
  });
  assert.equal(filtered.scanned, 24);
  assert.equal(filtered.rows.length, 2);
  assert.ok(filtered.rows[0].text.endsWith('0'));
  assert.ok(filtered.rows[1].text.endsWith('23'));
  const empty = await queryAll({
    file,
    limit,
    where: [{ path: '/text', op: 'contains', value: 'missing-sentinel' }],
  });
  assert.equal(empty.scanned, 24);
  assert.equal(empty.rows.length, 0);
  // Public workload is fixed per lifecycle/page arm; h1 retains the full selected scope.
  const publicFile = await extraction(connection, publicUrl, 'h1');
  const publicRows = await queryAll({ file: publicFile, limit });
  assert.ok(publicRows.rows.length > 0);
  const publicHash = hash(JSON.stringify(publicRows.rows));
  if (digests.public === null) digests.public = publicHash;
  assert.equal(
    publicHash,
    digests.public,
    'Public selected evidence changed between arms'
  );
  return {
    fixtureDigest: digest,
    completeRows: complete.rows.length,
    completePages: complete.pages,
    filteredRows: filtered.rows.length,
    filteredScanned: filtered.scanned,
    emptyRows: empty.rows.length,
    publicRows: publicRows.rows,
    publicDigest: publicHash,
  };
}
