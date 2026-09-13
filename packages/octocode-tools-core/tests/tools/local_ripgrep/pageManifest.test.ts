import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  unlinkSync,
  utimesSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import type { RipgrepQuery } from '@octocodeai/octocode-core/schema';
import {
  fingerprintLexicalResult,
  loadLexicalPageManifest,
  saveLexicalPageManifest,
} from '../../../src/tools/local_ripgrep/pageManifest.js';

const homes: string[] = [];
afterEach(() => {
  for (const home of homes.splice(0))
    rmSync(home, { recursive: true, force: true });
});

function fixture() {
  const home = mkdtempSync(join(tmpdir(), 'octocode-lexical-manifest-'));
  homes.push(home);
  const root = join(home, 'repo');
  mkdirSync(root);
  writeFileSync(join(root, 'a.ts'), 'needle\n');
  const query = {
    path: root,
    searchText: 'needle',
    page: 1,
    noIgnore: true,
  } as RipgrepQuery;
  const files = [
    {
      path: join(root, 'a.ts'),
      matchCount: 1,
      matches: [{ line: 1, column: 0, value: 'needle' }],
    },
  ];
  const stats = { filesSearched: 1, totalOccurrences: 1 };
  return { home, root, query, files, stats };
}

describe('immutable lexical page manifests', () => {
  it('fingerprints live results independently of page controls and scan telemetry', () => {
    const f = fixture();
    const token = fingerprintLexicalResult(f);
    const ordered = {
      ...f,
      files: [...f.files, { ...f.files[0]!, path: join(f.root, 'b.ts') }],
    };
    expect(
      fingerprintLexicalResult({
        ...ordered,
        files: [...ordered.files].reverse(),
      })
    ).not.toBe(fingerprintLexicalResult(ordered));
    expect(token).toMatch(/^lexical-live-v1:[a-f0-9]{64}$/);
    expect(
      fingerprintLexicalResult({
        ...f,
        query: {
          ...f.query,
          snapshot: token,
          page: 2,
          matchPage: 2,
          maxMatchesPerFile: 10,
        },
        stats: { ...f.stats, searchTime: 12, bytesSearched: 9000 },
      })
    ).toBe(token);
    expect(
      fingerprintLexicalResult({
        ...f,
        files: [
          {
            ...f.files[0]!,
            matches: [{ line: 2, column: 0, value: 'needle changed' }],
          },
        ],
      })
    ).not.toBe(token);
    expect(
      fingerprintLexicalResult({
        ...f,
        query: { ...f.query, searchText: 'different' },
      })
    ).not.toBe(token);
    expect(
      fingerprintLexicalResult({ ...f, stats: { ...f.stats, errorCount: 1 } })
    ).not.toBe(token);
  });
  it('loads the frozen result for later pages without changing page identity', async () => {
    const f = fixture();
    const saved = await saveLexicalPageManifest({ ...f, now: 1_000 });
    expect(saved.status).toBe('saved');
    if (saved.status !== 'saved') throw new Error('save failed');
    expect(
      await loadLexicalPageManifest({
        home: f.home,
        query: { ...f.query, page: 2, snapshot: saved.snapshot },
        root: f.root,
        now: 1_001,
      })
    ).toMatchObject({
      status: 'loaded',
      files: f.files,
      stats: f.stats,
    });
  });

  it('rejects expired and source-stale manifests with a typed restart reason', async () => {
    const f = fixture();
    let saved = await saveLexicalPageManifest({ ...f, now: 1_000, ttlMs: 10 });
    if (saved.status !== 'saved') throw new Error('save failed');
    expect(
      await loadLexicalPageManifest({
        home: f.home,
        query: { ...f.query, page: 2, snapshot: saved.snapshot },
        root: f.root,
        now: 1_011,
      })
    ).toMatchObject({ status: 'restart', reason: 'expired' });
    saved = await saveLexicalPageManifest({ ...f, now: 2_000 });
    if (saved.status !== 'saved') throw new Error('save failed');
    writeFileSync(join(f.root, 'a.ts'), 'changed-size\n');
    expect(
      await loadLexicalPageManifest({
        home: f.home,
        query: { ...f.query, page: 2, snapshot: saved.snapshot },
        root: f.root,
        now: 2_001,
      })
    ).toMatchObject({ status: 'restart', reason: 'sourceChanged' });
  });

  it('rejects manifest tampering and symlink substitution', async () => {
    const f = fixture();
    const saved = await saveLexicalPageManifest({ ...f, now: 1_000 });
    expect(saved.status).toBe('saved');
    if (saved.status !== 'saved') return;
    const doc = JSON.parse(readFileSync(saved.path, 'utf8'));
    doc.files[0].path = '/tampered';
    writeFileSync(saved.path, JSON.stringify(doc), { mode: 0o600 });
    expect(
      await loadLexicalPageManifest({
        home: f.home,
        query: { ...f.query, page: 2, snapshot: saved.snapshot },
        root: f.root,
        now: 1_001,
      })
    ).toMatchObject({ status: 'restart', reason: 'tampered' });

    const other = join(f.home, 'other.json');
    writeFileSync(other, '{}');
    writeFileSync(saved.path, '{}');
    unlinkSync(saved.path);
    symlinkSync(other, saved.path);
    expect(
      await loadLexicalPageManifest({
        home: f.home,
        query: { ...f.query, page: 2, snapshot: saved.snapshot },
        root: f.root,
        now: 1_001,
      })
    ).toMatchObject({ status: 'restart', reason: 'unsafeStorage' });
  });

  it('fails closed when the serialized result exceeds the size bound', async () => {
    const f = fixture();
    expect(
      await saveLexicalPageManifest({ ...f, now: 1_000, maxBytes: 32 })
    ).toMatchObject({ status: 'terminal', reason: 'manifestSizeLimit' });
  });
});

describe('complete lexical source freshness', () => {
  it.each(['add', 'delete', 'sameSize', 'nonmatching'] as const)(
    'rejects a %s mutation anywhere in scope',
    async mutation => {
      const f = fixture();
      const unmatched = join(f.root, 'other.ts');
      writeFileSync(unmatched, 'nothing\n');
      const saved = await saveLexicalPageManifest(f);
      if (saved.status !== 'saved') throw new Error('save failed');
      const file = join(f.root, 'a.ts');
      if (mutation === 'add') writeFileSync(join(f.root, 'new.ts'), 'needle\n');
      if (mutation === 'delete') unlinkSync(unmatched);
      if (mutation === 'nonmatching') writeFileSync(unmatched, 'needle\n');
      if (mutation === 'sameSize') {
        const stat = statSync(file);
        writeFileSync(file, 'xxxxxx\n');
        utimesSync(file, stat.atime, stat.mtime);
      }
      expect(
        await loadLexicalPageManifest({
          ...f,
          query: { ...f.query, snapshot: saved.snapshot },
        })
      ).toMatchObject({ status: 'restart', reason: 'sourceChanged' });
    }
  );

  it('does not freeze ignore-aware, incomplete, or symlink scopes', async () => {
    const f = fixture();
    expect(
      await saveLexicalPageManifest({
        ...f,
        query: { ...f.query, noIgnore: false },
      })
    ).toMatchObject({ status: 'terminal', reason: 'incompleteScope' });
    expect(
      await saveLexicalPageManifest({
        ...f,
        stats: { ...f.stats, errorCount: 1 },
      })
    ).toMatchObject({ status: 'terminal', reason: 'incompleteScope' });
    symlinkSync(join(f.root, 'a.ts'), join(f.root, 'alias.ts'));
    expect(await saveLexicalPageManifest(f)).toMatchObject({
      status: 'terminal',
      reason: 'sourceChanged',
    });
  });

  it('persists sanitized snippets only', async () => {
    const f = fixture();
    const secret = ['ghp_', 'AbCdEfGhIjKlMnOpQrStUvWxYz1234567890'].join('');
    f.files[0]!.matches[0]!.value = secret;
    const saved = await saveLexicalPageManifest(f);
    if (saved.status !== 'saved') throw new Error('save failed');
    expect(readFileSync(saved.path, 'utf8')).not.toContain(secret);
    expect(saved.files[0]!.matches![0]!.value).not.toBe(secret);
  });
});
