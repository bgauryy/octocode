import { describe, it, expect } from 'vitest';

import {
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LOCAL_OVERLAY_MAX_LIMIT,
  LOCAL_OVERLAY_MAX_DEPTH,
} from '../../src/scheme/localSchemaOverlay.js';
import { LSPCallHierarchyQuerySchema } from '../../src/scheme/lspSchemaOverlay.js';

describe('FindFilesQuerySchema.limit bound', () => {
  it('clamps limit above LOCAL_OVERLAY_MAX_LIMIT to the max', () => {
    const result = FindFilesQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(LOCAL_OVERLAY_MAX_LIMIT);
    }
  });

  it('clamps a negative limit up to the minimum', () => {
    const result = FindFilesQuerySchema.safeParse({
      path: '.',
      limit: -5,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(1);
    }
  });

  it('accepts limit at the max bound', () => {
    const result = FindFilesQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT,
    });
    expect(result.success).toBe(true);
  });

  it('accepts limit omitted', () => {
    const result = FindFilesQuerySchema.safeParse({ path: '.' });
    expect(result.success).toBe(true);
  });
});

describe('ViewStructureQuerySchema depth + limit bounds', () => {
  it('clamps depth above LOCAL_OVERLAY_MAX_DEPTH to the max', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      depth: LOCAL_OVERLAY_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(LOCAL_OVERLAY_MAX_DEPTH);
    }
  });

  it('clamps limit above LOCAL_OVERLAY_MAX_LIMIT to the max', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(LOCAL_OVERLAY_MAX_LIMIT);
    }
  });

  it('clamps a negative depth up to the minimum', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      depth: -1,
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(0);
    }
  });

  it('accepts depth at the max bound', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      depth: LOCAL_OVERLAY_MAX_DEPTH,
    });
    expect(result.success).toBe(true);
  });
});

describe('LSPCallHierarchyQuerySchema depth bound', () => {
  const base = {
    uri: 'file:///x',
    line: 1,
    character: 1,
  };

  it('rejects depth above LOCAL_OVERLAY_MAX_DEPTH', () => {
    const result = LSPCallHierarchyQuerySchema.safeParse({
      ...base,
      depth: LOCAL_OVERLAY_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(false);
  });

  it('rejects negative depth', () => {
    const result = LSPCallHierarchyQuerySchema.safeParse({
      ...base,
      depth: -1,
    });
    expect(result.success).toBe(false);
  });

  it('accepts depth at the max bound', () => {
    const result = LSPCallHierarchyQuerySchema.safeParse({
      ...base,
      depth: LOCAL_OVERLAY_MAX_DEPTH,
    });
    // Result may still fail validation on other required fields; what we
    // assert is that depth is NOT the offender.
    if (!result.success) {
      const paths = result.error.issues.map(i => i.path.join('.'));
      expect(paths).not.toContain('depth');
    }
  });
});
