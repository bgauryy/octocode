import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nativeResults, verifyContributionReads } from '../../src/collaboration-evidence.mjs';

function fixture() {
  const old = { name: 'author-coordination.md', author: 'author', sha256: 'old', bytes: Buffer.byteLength('Verified evidence: COPPER.') };
  const current = { name: 'author-coordination-v2.md', author: 'author', sha256: 'new', bytes: Buffer.byteLength('Verified evidence: COPPER.') };
  const publication = document => ({ name: 'share_document', value: { document } });
  const read = document => ({ name: 'read_document', value: { document, offset: 0, content: 'Verified evidence: COPPER.' } });
  return { requests: [{ id: 7, sender: 'author', recipient: 'reader', body: 'Review author-coordination-v2.md.' }],
    records: new Map([['author', [publication(old), publication(current), publication(current)]], ['reader', [read(current)]]]), old, current, read };
}
test('a corrected immutable document is verified by the requested revision, author and hash', () => {
  const f = fixture();
  assert.deepEqual(verifyContributionReads(f.requests, f.records), [{ request: 7, author: 'author', reader: 'reader', name: f.current.name, sha256: 'new' }]);
});
test('an older read, missing read or mismatched content hash cannot satisfy the request', () => {
  for (const kind of ['old', 'missing', 'hash', 'proof']) {
    const f = fixture();
    f.records.set('reader', kind === 'missing' ? [] : [f.read(kind === 'old' ? f.old : { ...f.current, ...(kind === 'hash' ? { sha256: 'different' } : {}) })]);
    if (kind === 'proof') f.records.get('reader')[0].value.content = 'No source proof';
    assert.throws(() => verifyContributionReads(f.requests, f.records), /must read/);
  }
});
test('foreign publications, ambiguous references and substring lookalikes do not pass', () => {
  for (const kind of ['foreign', 'ambiguous', 'substring']) {
    const f = fixture();
    if (kind === 'foreign') for (const record of f.records.get('author')) record.value.document.author = 'someone-else';
    if (kind === 'ambiguous') f.requests[0].body += ' Also author-coordination.md';
    if (kind === 'substring') f.requests[0].body = 'Review other-author-coordination-v2.md';
    assert.throws(() => verifyContributionReads(f.requests, f.records), /exactly one/);
  }
});

test('Codex application-level tool errors never count as successful evidence', () => {
  const event = { method: 'item/completed', params: { item: { type: 'mcpToolCall', status: 'completed', tool: 'read_document', result: { isError: true, content: [{ text: JSON.stringify({ document: fixture().current, content: 'COPPER' }) }] } } } };
  assert.deepEqual(nativeResults({ vendor: 'codex', rpc: { events: [event] } }), []);
});

test('document proof requires contiguous pages through the terminal byte, not one matching page', () => {
  const f = fixture();
  const content = 'Verified evidence: COPPER.';
  const page = (offset, text, next) => ({ name: 'read_document', value: { document: f.current, offset, content: text, ...(next ? { next } : {}) } });
  for (const pages of [
    [page(19, 'COPPER.')],
    [page(0, content.slice(0, 20), { offset: 20 })],
    [page(0, content.slice(0, 10), { offset: 10 }), page(19, content.slice(19))],
    [page(0, content, { offset: content.length })],
  ]) {
    f.records.set('reader', pages);
    assert.throws(() => verifyContributionReads(f.requests, f.records), /must read all/);
  }
  f.records.set('reader', [page(0, content.slice(0, 21), { offset: 21 }), page(21, content.slice(21))]);
  assert.equal(verifyContributionReads(f.requests, f.records)[0].name, f.current.name);
});

test('Pi native success receipts count and either tool-error marker excludes evidence', () => {
  const result = { content: [{ text: JSON.stringify({ renewed: true }) }] };
  const success = { type: 'tool_execution_end', toolName: 'renew', result };
  assert.deepEqual(nativeResults({ vendor: 'pi', process: { events: [success] } }), [{ name: 'renew', value: { renewed: true } }]);
  for (const failure of [{ ...success, isError: true }, { ...success, result: { ...result, isError: true } }]) {
    assert.deepEqual(nativeResults({ vendor: 'pi', process: { events: [failure] } }), []);
  }
});
