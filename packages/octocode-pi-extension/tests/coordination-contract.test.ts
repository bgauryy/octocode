import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { renderCasefoldTable, SOURCE, TARGET } from '../scripts/generate-casefold.mjs';
import { CASE_FOLDING_UNICODE_VERSION } from '../src/team/casefold-table.generated.js';
import { foldComponent, keysOverlap, leaseKey, type LeaseKind } from '../src/team/lease-key.js';
import { pathKey, TeamStore } from '../src/team/store.js';
import type { Member } from '../src/team/model.js';
import { repo, tmp } from './helpers.js';

/**
 * The reservation rules Pi's team shares with the octocode-agents-communication package. The vectors live in
 * test-utils/agent-coordination/contract.json; the package's own suite runs the same file against its TypeScript runtime
 * (packages/octocode-agents-communication/tests/coordination-contract.test.ts).
 */
interface Spot { path: string; kind: LeaseKind }
interface Contract {
  fold: Array<{ name: string; component: string; folded: string }>;
  keys: Array<{ name: string; path: string; key: string }>;
  overlap: Array<{ name: string; a: Spot; b: Spot; overlap: boolean }>;
  batches: Array<{ name: string; paths: Spot[]; refused: boolean }>;
}
const contract = JSON.parse(fs.readFileSync(new URL('../../../test-utils/agent-coordination/contract.json', import.meta.url), 'utf8')) as Contract;

const stores: TeamStore[] = [];
afterEach(() => {
  for (const store of stores.splice(0)) store.close();
});

function open() {
  const cwd = repo();
  const store = TeamStore.open(cwd, path.join(tmp(), 'team.sqlite'), {});
  stores.push(store);
  for (const id of ['one', 'two']) store.save({ id, role: id, pid: process.pid, status: 'idle', joinedAt: 1_000, updatedAt: Date.now(), toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false } as Member);
  return { cwd, store };
}

const request = ({ path: file, kind }: Spot) => ({ path: file, kind, reason: 'contract' });

describe('agent coordination contract (shared vectors)', () => {
  it('has vectors for every rule the README names', () => {
    const names = contract.fold.map((vector) => vector.name).join('\n');
    for (const needed of ['sharp s', 'final sigma', 'turkish', 'combining', 'ligature fi']) expect(names).toContain(needed);
    expect(contract.overlap.some((vector) => vector.overlap)).toBe(true);
    expect(contract.overlap.some((vector) => !vector.overlap)).toBe(true);
    expect(contract.batches.some((vector) => vector.refused)).toBe(true);
  });

  it.each(contract.fold.map((vector) => [vector.name, vector] as const))('folds a component: %s', (_name, vector) => {
    expect(foldComponent(vector.component)).toBe(vector.folded);
  });

  it.each(contract.keys.map((vector) => [vector.name, vector] as const))('keys a path: %s', (_name, vector) => {
    expect(leaseKey(vector.path.split('/'))).toBe(vector.key);
    // The workspace-relative key Pi stores is the same key.
    expect(pathKey(fs.realpathSync(tmp()), vector.path)).toBe(vector.key);
  });

  it.each(contract.overlap.map((vector) => [vector.name, vector] as const))('overlaps: %s', (_name, vector) => {
    const key = ({ path: file }: Spot) => leaseKey(file.split('/'));
    expect(keysOverlap(key(vector.a), vector.a.kind, key(vector.b), vector.b.kind)).toBe(vector.overlap);
    // And through the team store: one agent holds `a`, so `b` conflicts for the other exactly when they overlap.
    const { store } = open();
    expect(store.lock('one', [request(vector.a)]).ok).toBe(true);
    expect(store.conflict(vector.b.path, 'two', vector.b.kind) !== undefined).toBe(vector.overlap);
    expect(store.lock('two', [request(vector.b)]).ok).toBe(!vector.overlap);
  });

  it.each(contract.batches.map((vector) => [vector.name, vector] as const))('one call asking for: %s', (_name, vector) => {
    const { store } = open();
    if (!vector.refused) {
      expect(store.lock('one', vector.paths.map(request)).ok).toBe(true);
      return;
    }
    expect(() => store.lock('one', vector.paths.map(request))).toThrow(/Requested paths overlap: .* Nothing was locked/);
    expect(store.leases()).toEqual([]);
  });

  it('keeps the generated case-folding table equal to the package\'s frozen Unicode table', () => {
    const table = JSON.parse(fs.readFileSync(SOURCE, 'utf8')) as { version: string; casefold: Record<string, number[]> };
    expect(CASE_FOLDING_UNICODE_VERSION).toBe(table.version);
    expect(fs.readFileSync(TARGET, 'utf8'), 'src/team/casefold-table.generated.ts is stale: run `yarn casefold:generate` in packages/octocode-pi-extension').toBe(renderCasefoldTable(table));
  });
});
