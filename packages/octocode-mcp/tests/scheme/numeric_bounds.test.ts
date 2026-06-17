import { describe, it, expect } from 'vitest';

import {
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_LIMIT,
} from '../../../octocode-tools-core/src/config.js';

const LOCAL_OVERLAY_MAX_LIMIT = LOCAL_MAX_LIMIT;
const LOCAL_OVERLAY_MAX_DEPTH = LOCAL_MAX_DEPTH;
import { LocalFindFilesQuerySchema } from '../../../octocode-tools-core/src/tools/local_find_files/scheme.js';
import { LocalViewStructureQuerySchema } from '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js';
import { LspGetSemanticsQuerySchema } from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';

describe('LocalFindFilesQuerySchema.limit bound', () => {
  it('clamps limit above LOCAL_OVERLAY_MAX_LIMIT to the max', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(LOCAL_OVERLAY_MAX_LIMIT);
    }
  });

  it('clamps a negative limit up to the minimum', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      path: '.',
      limit: -5,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(1);
    }
  });

  it('accepts limit at the max bound', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT,
    });
    expect(result.success).toBe(true);
  });

  it('accepts limit omitted', () => {
    const result = LocalFindFilesQuerySchema.safeParse({ path: '.' });
    expect(result.success).toBe(true);
  });
});

describe('LocalViewStructureQuerySchema depth + limit bounds', () => {
  it('clamps maxDepth above LOCAL_OVERLAY_MAX_DEPTH to the max', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      path: '.',
      maxDepth: LOCAL_OVERLAY_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.maxDepth).toBe(LOCAL_OVERLAY_MAX_DEPTH);
    }
  });

  it('clamps limit above LOCAL_OVERLAY_MAX_LIMIT to the max', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(LOCAL_OVERLAY_MAX_LIMIT);
    }
  });

  it('clamps a negative maxDepth up to the minimum', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      path: '.',
      maxDepth: -1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.maxDepth).toBe(0);
    }
  });

  it('accepts maxDepth at the max bound', () => {
    const result = LocalViewStructureQuerySchema.safeParse({
      path: '.',
      maxDepth: LOCAL_OVERLAY_MAX_DEPTH,
    });
    expect(result.success).toBe(true);
  });
});

describe('LspGetSemanticsQuerySchema depth bound', () => {
  const base = {
    uri: '/tmp/x.ts',
    type: 'callers',
    symbolName: 'x',
    lineHint: 1,
  };

  it('clamps depth above LOCAL_OVERLAY_MAX_DEPTH to the max', () => {
    const result = LspGetSemanticsQuerySchema.safeParse({
      ...base,
      depth: LOCAL_OVERLAY_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(LOCAL_OVERLAY_MAX_DEPTH);
    }
  });

  it('clamps negative depth up to the minimum', () => {
    const result = LspGetSemanticsQuerySchema.safeParse({
      ...base,
      depth: -1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(0);
    }
  });

  it('accepts depth at the max bound', () => {
    const result = LspGetSemanticsQuerySchema.safeParse({
      ...base,
      depth: LOCAL_OVERLAY_MAX_DEPTH,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(LOCAL_OVERLAY_MAX_DEPTH);
    }
  });
});
