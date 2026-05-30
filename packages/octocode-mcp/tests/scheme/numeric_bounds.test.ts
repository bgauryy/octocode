import { describe, it, expect } from 'vitest';

import {
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  LOCAL_OVERLAY_MAX_LIMIT,
  LOCAL_OVERLAY_MAX_DEPTH,
} from '../../src/scheme/localSchemaOverlay.js';
import { LSPCallHierarchyQuerySchema } from '../../src/scheme/lspSchemaOverlay.js';

describe('FindFilesQuerySchema.limit bound', () => {
  it('rejects limit above LOCAL_OVERLAY_MAX_LIMIT', () => {
    const result = FindFilesQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(false);
  });

  it('rejects negative limit', () => {
    const result = FindFilesQuerySchema.safeParse({
      path: '.',
      limit: -5,
    });
    expect(result.success).toBe(false);
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
  it('rejects depth above LOCAL_OVERLAY_MAX_DEPTH', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      depth: LOCAL_OVERLAY_MAX_DEPTH + 1,
    });
    expect(result.success).toBe(false);
  });

  it('rejects limit above LOCAL_OVERLAY_MAX_LIMIT', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      limit: LOCAL_OVERLAY_MAX_LIMIT + 1,
    });
    expect(result.success).toBe(false);
  });

  it('rejects negative depth', () => {
    const result = ViewStructureQuerySchema.safeParse({
      path: '.',
      depth: -1,
    });
    expect(result.success).toBe(false);
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
