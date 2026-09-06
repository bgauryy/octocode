import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { LspGetSemanticsQuerySchema } from '../../../src/tools/lsp/semantic_content/scheme.js';
import {
  describeRustContext,
  semanticSnapshotItems,
} from '../../../src/tools/lsp/semantic_content/semanticSnapshot.js';
import type { LspGetSemanticsQuery } from '../../../src/tools/lsp/shared/semanticTypes.js';
import { attachReadinessWarning } from '../../../src/tools/lsp/shared/readiness.js';

const query = {
  uri: '/workspace/main.rs',
  type: 'references' as const,
  symbolName: 'target',
  lineHint: 1,
};

describe('public Rust semantic contexts', () => {
  it('marks an unconfirmed empty answer as typed partial evidence', () => {
    const envelope = {
      type: 'definition' as const,
      uri: query.uri,
      lsp: { serverAvailable: true },
      payload: {
        kind: 'empty' as const,
        category: 'noLocations' as const,
        reason: 'No locations',
      },
    };
    expect(
      attachReadinessWarning(envelope, 'timeout').partialReasons
    ).toContain('readinessUnconfirmed');
    expect(attachReadinessWarning(envelope, 'timeout').incompleteResults).toBe(
      true
    );
    expect(
      attachReadinessWarning(envelope, 'progressIdle').partialReasons
    ).toBeUndefined();
  });
  it('can publish the public context as JSON Schema', () => {
    expect(() => z.toJSONSchema(LspGetSemanticsQuerySchema)).not.toThrow();
  });
  it('normalizes equivalent contexts and requires explicit execution permission', () => {
    const first = LspGetSemanticsQuerySchema.parse({
      ...query,
      rustContext: { features: ['b', 'a', 'a'], cfgs: ['z', 'a'] },
    });
    const second = LspGetSemanticsQuerySchema.parse({
      ...query,
      rustContext: { cfgs: ['a', 'z'], features: ['a', 'b'] },
    });
    expect(describeRustContext(first)).toEqual(describeRustContext(second));
    expect(first.rustContext).toMatchObject({
      buildScripts: false,
      procMacros: false,
    });
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        ...query,
        rustContext: { procMacros: true },
      }).success
    ).toBe(false);
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        ...query,
        rustContext: { procMacros: true, buildScripts: true },
      }).success
    ).toBe(true);
    expect(
      LspGetSemanticsQuerySchema.safeParse({
        ...query,
        uri: '/workspace/main.ts',
        rustContext: {},
      }).success
    ).toBe(false);
  });

  it('never reuses a snapshot across build contexts even when locations coincide', () => {
    const rows = [
      { uri: query.uri, range: { start: { line: 1, character: 0 } } },
    ];
    const base = {
      ...query,
      rustContext: { features: ['a'] },
    } as LspGetSemanticsQuery;
    const first = semanticSnapshotItems(rows, base).snapshot;
    expect(
      semanticSnapshotItems(rows, { ...base, rustContext: { features: ['b'] } })
        .snapshot
    ).not.toBe(first);
    expect(semanticSnapshotItems(rows, { ...base, page: 2 }).snapshot).toBe(
      first
    );
    expect(describeRustContext(base)?.fingerprint).toMatch(/^rust-v1:/);
  });
});
