import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rankResources, runLocalSemantic } from '../src/local-semantic.mjs';
import { selectMarkdownSections } from '../src/sections.mjs';

test('page ranking retains partial coverage, uncertainty, and source ranges', () => {
  const result = rankResources([
    { path: '/a', coverage: 'partial', pages: [
      { scope: { startLine: 1, endLine: 80 }, answers: { q: { noul: 0.2 } } },
      { scope: { startLine: 81, endLine: 160 }, answers: { q: { noul: 0.9 } } }
    ] },
    { path: '/b', coverage: 'complete', pages: [{ answers: { q: { noul: 0.7 } } }] },
    { path: '/c', coverage: 'error', error: 'provider unavailable', pages: [] }
  ], 'q');
  assert.equal(result.length, 3);
  assert.equal(result[0].coverage, 'partial');
  assert.equal(result[0].bestPageValue, 0.9);
  assert.equal(result[0].read.query.startLine, 81);
  assert.equal(result[0].read.query.endLine, 160);
  assert.equal(result[1].read.query.startLine, undefined);
  assert.equal(result[2].bestPageValue, null);
  assert.equal(result[2].error, 'provider unavailable');
});

test('Choice ranks the requested probability, not the winning label or confidence', () => {
  const result = rankResources([
    { path: '/a', coverage: 'complete', pages: [{ answers: { q: { choice: 'irrelevant', confidence: 0.95, probabilities: { useful: 0.05, irrelevant: 0.95 } } } }] },
    { path: '/b', coverage: 'complete', pages: [{ answers: { q: { choice: 'useful', confidence: 0.6, probabilities: { useful: 0.6, irrelevant: 0.4 } } } }] }
  ], 'q', 'useful');
  assert.equal(result[0].path, '/b');
  assert.equal(result[0].bestPageValue, 0.6);
  assert.equal(result[0].answers.q.probabilities.irrelevant, 0.4);
});

test('Score keeps its native scale instead of pretending to be P(yes)', () => {
  const result = rankResources([{ path: '/a', coverage: 'complete', pages: [{ answers: { q: { score: 2.4, probabilities: [0.1, 0.1, 0.1, 0.7] } } }] }], 'q');
  assert.equal(result[0].bestPageValue, 2.4);
});

test('questions select their own assessed page and preserve evidence identity', () => {
  const resource = { path: '/a', coverage: 'complete', pages: [
    { source: { evidenceHash: 'a'.repeat(64) }, scope: { startLine: 1, endLine: 100 }, answers: { isolation: { noul: 0.9 }, shutdown: { noul: 0.1 } } },
    { source: { evidenceHash: 'b'.repeat(64) }, scope: { startLine: 101, endLine: 200 }, answers: { isolation: { noul: 0.1 }, shutdown: { noul: 0.95 } } }
  ] };
  assert.equal(rankResources([resource], 'isolation')[0].read.query.startLine, 1);
  const result = rankResources([resource], 'shutdown');
  assert.equal(result[0].read.query.startLine, 101);
  assert.equal(result[0].source.evidenceHash, 'b'.repeat(64));
});

test('section selection includes nested headings, ends at a peer, and avoids overlap', () => {
  const outline = '   3|   ## Hooks\n 467|     ### onClose\n 493|       #### Execution order\n 515|     ### preClose\n 557|     ### onRoute';
  assert.deepEqual(selectMarkdownSections(outline, 954, 'onClose|Execution order|preClose'), [
    { title: 'onClose', startLine: 467, endLine: 514 },
    { title: 'preClose', startLine: 515, endLine: 556 }
  ]);
  assert.deepEqual(selectMarkdownSections(outline, 954, 'absent'), []);
  assert.throws(() => selectMarkdownSections('20| ## A\n3| ## B', 30, '.'), /not ordered/);
});

test('exact duplicates share a classification while retaining distinct source identities', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'semantic-poc-'));
  try {
    await writeFile(join(dir, 'a.md'), 'same source');
    await writeFile(join(dir, 'b.md'), 'same source');
    await writeFile(join(dir, 'c.md'), 'different source');
    const seen = [];
    const client = { async callTool({ name, arguments: args }) {
      seen.push({ name, args });
      if (name === 'astSearch') return { structuredContent: { base: dir, results: [{ data: { files: ['a.md', 'b.md', 'c.md'].map(path => ({ path })), pagination: { totalFiles: 3, hasMore: false } } }] } };
      return { structuredContent: { queries: [{ usage: { input_tokens: 10, output_tokens: 2 }, resources: args.resources.map(r => ({ resourceId: r.id, coverage: 'complete', pages: [{ scope: { startLine: 1, endLine: 1 }, answers: { q: { noul: 0.8 } } }] })) }] } };
    } };
    const result = await runLocalSemantic(client, { path: dir, dedupe: true, questions: [{ id: 'q', question: { type: 'noul', instructions: 'Relevant?' } }] });
    assert.equal(result.scannedFiles, 3);
    assert.equal(result.classifiedFiles, 2);
    assert.equal(result.candidates.length, 3);
    assert.equal(seen[1].args.resources.length, 2);
    const duplicate = result.candidates.find(r => r.path.endsWith('b.md'));
    assert.equal(duplicate.duplicateOf, join(dir, 'a.md'));
    assert.equal(duplicate.read.query.path, join(dir, 'b.md'));
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('outline coordinates cannot be reused as original source line numbers', () => {
  const result = rankResources([{ path: '/a', view: 'symbols', coverage: 'complete', pages: [{ scope: { startLine: 1, endLine: 20, totalLines: 1000 }, answers: { q: { noul: 0.8 } } }] }], 'q');
  assert.equal(result[0].read.query.startLine, undefined);
  assert.equal(result[0].view, 'symbols');
});

test('disjoint source ranges remain separate executable reads', () => {
  const result = rankResources([{ path: '/a', coverage: 'complete', pages: [{ scope: { lineRanges: [{ startLine: 10, endLine: 20 }, { startLine: 200, endLine: 210 }] }, answers: { q: { noul: 0.9 } } }] }], 'q');
  assert.deepEqual(result[0].reads.map(r => [r.query.startLine, r.query.endLine]), [[10, 20], [200, 210]]);
});

test('section screening keeps missing headings and earlier failed regions unresolved', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'semantic-sections-'));
  try {
    for (const name of ['a.md', 'b.md']) await writeFile(join(dir, name), '# Intro\ntext\n## Close\nclose\n## Stop\nstop\n');
    const client = { async callTool({ name, arguments: args }) {
      if (name === 'astSearch') return { structuredContent: { base: dir, results: [{ data: { files: [{ path: 'a.md' }, { path: 'b.md' }], pagination: { totalFiles: 2 } } }] } };
      if (name === 'localFetch') return { structuredContent: { results: [{ data: { contentView: 'symbols', totalLines: 6, content: args.queries[0].path.endsWith('a.md') ? '1| # Intro\n3| ## Close\n5| ## Stop' : '1| # Intro' } }] } };
      assert.deepEqual(args.resources.map(r => [r.context.query.startLine, r.context.query.endLine]), [[3, 4], [5, 6]]);
      return { structuredContent: { queries: [{ resources: args.resources.map((r, i) => ({ resourceId: r.id, coverage: i ? 'complete' : 'error', ...(i ? {} : { error: 'read failed' }), pages: i ? [{ answers: { q: { noul: 0.8 } } }] : [] })) }] } };
    } };
    const result = await runLocalSemantic(client, { path: dir, sectionPattern: 'Close|Stop', questions: [{ id: 'q', question: { type: 'noul', instructions: 'Relevant?' } }] });
    assert.equal(result.candidates.find(r => r.path.endsWith('a.md')).coverage, 'partial');
    assert.equal(result.candidates[0].selectionScope, 'selected-sections');
    assert.equal(result.unresolved.length, 2);
    assert.equal(result.unresolved.find(r => r.path.endsWith('b.md')).coverage, 'unscanned');
  } finally { await rm(dir, { recursive: true, force: true }); }
});
