import { describe, expect, it } from 'vitest';

import {
  findTransformerEntry,
  listTransformerEntries,
} from '../../src/oql/transformers/registry.js';

describe('OQL transformer registry', () => {
  it('has unique transformer ids', () => {
    const ids = listTransformerEntries().map(entry => entry.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('maps GitHub code to the active ghSearchCode transformer', () => {
    const entry = findTransformerEntry({
      sourceKind: 'github',
      target: 'code',
    });
    expect(entry).toMatchObject({
      id: 'github.code',
      status: 'active',
      backends: [{ backend: 'ghSearchCode', operation: 'searchCode' }],
      adapterFunctions: ['toGithubCodeSearchToolQuery'],
    });
  });

  it('covers the major inline adapter lanes', () => {
    const ids = new Set(listTransformerEntries().map(entry => entry.id));
    for (const id of [
      'github.content',
      'github.structure',
      'github.semantics',
      'local.files',
      'local.content',
      'github.repositories',
      'npm.packages',
      'local.diff.directFile',
      'local.research',
      'local.graph',
    ]) {
      expect(ids.has(id)).toBe(true);
    }
  });

  it('marks GitHub file discovery as approximate provider semantics', () => {
    expect(
      findTransformerEntry({
        sourceKind: 'github',
        target: 'files',
      })?.backends[0]
    ).toMatchObject({
      backend: 'ghSearchCode',
      operation: 'findFiles',
      exact: false,
    });
  });

  it('uses runtime adapter provenance names for smart research lanes', () => {
    expect(
      findTransformerEntry({
        sourceKind: 'local',
        target: 'research',
      })?.backends[0]?.backend
    ).toBe('smartOqlResearch');
    expect(
      findTransformerEntry({
        sourceKind: 'local',
        target: 'graph',
      })?.backends[0]?.backend
    ).toBe('smartOqlGraph');
  });

  it('models remote semantics as materialize then LSP', () => {
    const entry = findTransformerEntry({
      sourceKind: 'github',
      target: 'semantics',
    });
    expect(entry?.backends.map(b => b.backend)).toEqual([
      'ghCloneRepo',
      'lspGetSemantics',
    ]);
    expect(entry?.adapterFunctions).toEqual(['executeSemantics']);
  });

  it('points materialize at the checkpoint adapter', () => {
    const entry = findTransformerEntry({
      sourceKind: 'github',
      target: 'materialize',
    });
    expect(entry).toMatchObject({
      adapterModule: 'adapters/materialize.ts',
      adapterFunctions: ['executeMaterializeCheckpoint'],
    });
  });

  it('selects both diff transformer variants', () => {
    expect(
      findTransformerEntry({
        sourceKind: 'github',
        target: 'diff',
        variant: 'prPatch',
      })?.backends[0]?.backend
    ).toBe('ghHistoryResearch');
    expect(
      findTransformerEntry({
        sourceKind: 'github',
        target: 'diff',
        variant: 'directFile',
      })?.backends[0]?.backend
    ).toBe('ghGetFileContent');
    expect(
      findTransformerEntry({
        sourceKind: 'local',
        target: 'diff',
        variant: 'directFile',
      })?.backends[0]?.backend
    ).toBe('localGetFileContent');
  });

  it('every implemented entry names a backend and adapter function', () => {
    for (const entry of listTransformerEntries()) {
      expect(entry.backends.length).toBeGreaterThan(0);
      expect(entry.adapterFunctions.length).toBeGreaterThan(0);
    }
  });
});
