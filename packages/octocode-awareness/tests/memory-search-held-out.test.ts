import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { initDb } from '../src/db-init.js';
import { queryMemory } from '../src/memory-recall.js';
import {
  anyReferenceCandidateIds,
  attachMemoryReferences,
  compileRecallRegex,
  exactReferenceCandidateIds,
  fileReferenceCandidates,
  fileReferenceMatchesToken,
  fileRegexCandidateIds,
  fileSuffixCandidateIds,
  fileSuffixTokens,
  intersectCandidateIds,
  lexicalSearch,
  regexCandidateIds,
} from '../src/memory-search.js';
import { insertMemory } from '../src/memory-write.js';
import { atomicWriteText, resolveWorkspaceOutputPath } from '../src/repo-projection.js';
import type { MemoryRecord } from '../src/types/identity-memory.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

async function memoryFixture() {
  const db = freshDb();
  const first = await insertMemory(db, {
    agentId: 'owner', taskContext: 'cache invalidation',
    observation: 'Invalidate the parser cache after writes', importance: 8,
    label: 'GOTCHA', tags: ['cache', 'parser'], workspacePath: '/workspace',
    artifact: 'core', repo: 'owner/repo', ref: 'main', failureSignature: 'cache-stale',
    references: ['file:/workspace/src/parser/cache.ts:42', 'issue:123'],
  });
  const second = await insertMemory(db, {
    agentId: 'owner', taskContext: 'unrelated renderer',
    observation: 'Render the document footer', importance: 3,
    label: 'DOCS', tags: ['render'], workspacePath: '/workspace',
    references: ['file:/workspace/src/render/footer.ts'],
  });
  return { db, first: first.memoryId, second: second.memoryId };
}

describe('memory search held-out filters', () => {
  it('combines exact, suffix, file-regex, and whole-record regex filters', async () => {
    const { db, first } = await memoryFixture();
    const result = queryMemory(db, {
      query: 'cache', files: ['parser/cache.ts'], references: ['issue:123'],
      fileRegex: ['parser/cache\\.ts'], regex: ['cache-stale', 'GOTCHA'],
      workspacePath: '/workspace', strictScope: true, recordAccess: false, explain: true,
    });
    expect(result.memories.map(memory => memory.memory_id)).toEqual([first]);
    expect(result.applied_filters).toMatchObject({ references: ['issue:123'] });

    expect(() => queryMemory(db, { regex: ['['], recordAccess: false }))
      .toThrow(/invalid regex/);
    db.close();
  });

  it('covers candidate intersections and every reference matching mode', async () => {
    const { db, first, second } = await memoryFixture();
    expect(intersectCandidateIds(null, new Set([first, second])))
      .toEqual(new Set([first, second]));
    expect(intersectCandidateIds(new Set([first, 'missing']), new Set([first, second])))
      .toEqual(new Set([first]));

    expect(exactReferenceCandidateIds(db, [])).toEqual(new Set());
    expect(exactReferenceCandidateIds(db, ['issue:123', 'file:/workspace/src/parser/cache.ts:42']))
      .toEqual(new Set([first]));
    expect(anyReferenceCandidateIds(db, [])).toEqual(new Set());
    expect(anyReferenceCandidateIds(db, ['issue:123', 'missing'])).toEqual(new Set([first]));

    expect(fileSuffixCandidateIds(db, [])).toEqual(new Set());
    expect(fileSuffixCandidateIds(db, ['parser/cache.ts'])).toEqual(new Set([first]));
    expect(fileRegexCandidateIds(db, [])).toEqual(new Set());
    expect(fileRegexCandidateIds(db, [/parser/, /cache\.ts/])).toEqual(new Set([first]));
    expect(regexCandidateIds(db, [])).toEqual(new Set());
    expect(regexCandidateIds(db, [/cache-stale/, /issue:123/])).toEqual(new Set([first]));
    db.close();
  });

  it('normalizes file candidates and safely handles missing reference storage', () => {
    expect(compileRecallRegex('cache.+ts')).toBeInstanceOf(RegExp);
    expect(() => compileRecallRegex('[')).toThrow(/invalid regex/);
    expect(fileReferenceCandidates(['', 'file:src/a.ts', './src/b.ts'], '/workspace'))
      .toEqual(expect.arrayContaining([
        'file:src/a.ts', 'src/a.ts', './src/b.ts', 'file:./src/b.ts',
        '/workspace/src/b.ts', 'file:/workspace/src/b.ts',
      ]));
    expect(fileSuffixTokens(['', 'file:./src\\a.ts:12', './b.ts', 'b.ts']))
      .toEqual(['src/a.ts', 'b.ts']);
    expect(fileReferenceMatchesToken('https://example.test', 'a.ts')).toBe(false);
    expect(fileReferenceMatchesToken('file:', 'a.ts')).toBe(false);
    expect(fileReferenceMatchesToken('file:/workspace/src/a.ts:9', 'src/a.ts')).toBe(true);
    expect(fileReferenceMatchesToken('file:a.ts', '/workspace/a.ts')).toBe(true);

    const db = new DatabaseSync(':memory:');
    const memory = { memory_id: 'mem_missing_refs', references: ['stale'] } as unknown as MemoryRecord;
    expect(() => attachMemoryReferences(db, [])).not.toThrow();
    expect(() => attachMemoryReferences(db, [memory])).not.toThrow();
    expect(memory.references).toEqual(['stale']);
    db.close();
  });

  it('uses and cleans the large candidate-id table', async () => {
    const { db, first } = await memoryFixture();
    const ids = [first, ...Array.from({ length: 401 }, (_, index) => `mem_missing_${index}`)];
    expect(lexicalSearch(db, '', 10, 1, [], [], ['ACTIVE'], { candidateMemoryIds: ids, allWorkspaces: true }))
      .toEqual([expect.objectContaining({ memory_id: first })]);
    expect(db.prepare('SELECT COUNT(*) AS count FROM temp_memory_candidate_ids').get())
      .toEqual({ count: 0 });
    expect(lexicalSearch(db, '', 10, 1, [], [], ['ACTIVE'], { candidateMemoryIds: [] }))
      .toEqual([]);
    db.close();
  });
});

describe('small filesystem projections', () => {
  it('resolves output paths and writes text atomically', () => {
    const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-projection-')));
    roots.push(root);
    expect(resolveWorkspaceOutputPath(undefined, root, 'default/report.md'))
      .toBe(resolve(root, 'default/report.md'));
    expect(resolveWorkspaceOutputPath('  custom/report.md  ', root, 'ignored'))
      .toBe(resolve(root, 'custom/report.md'));
    expect(resolveWorkspaceOutputPath(resolve(root, 'absolute.md'), root, 'ignored'))
      .toBe(resolve(root, 'absolute.md'));

    const output = join(root, 'nested', 'report.md');
    atomicWriteText(output, 'first');
    atomicWriteText(output, 'second');
    expect(readFileSync(output, 'utf8')).toBe('second');
  });
});
