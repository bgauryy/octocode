import { describe, it, expect } from 'vitest';
import {
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';

const baseMeta = {
  id: 'q:1',
  mainResearchGoal: 'Test',
  researchGoal: 'Test',
  reasoning: 'Test',
  symbolName: 'myFn',
  lineHint: 10,
};

describe('lspGotoDefinition — filePath alias', () => {
  it('accepts filePath as alias for uri', () => {
    const result = LSPGotoDefinitionQuerySchema.safeParse({
      ...baseMeta,
      filePath: '/src/foo.ts',
    });
    expect(result.success).toBe(true);
    expect(result.data?.uri).toBe('/src/foo.ts');
  });

  it('uri takes precedence over filePath when both are provided', () => {
    const result = LSPGotoDefinitionQuerySchema.safeParse({
      ...baseMeta,
      uri: '/src/bar.ts',
      filePath: '/src/foo.ts',
    });
    expect(result.success).toBe(true);
    expect(result.data?.uri).toBe('/src/bar.ts');
  });

  it('rejects input with neither uri nor filePath', () => {
    const result = LSPGotoDefinitionQuerySchema.safeParse({ ...baseMeta });
    expect(result.success).toBe(false);
  });

  it('still accepts uri directly', () => {
    const result = LSPGotoDefinitionQuerySchema.safeParse({
      ...baseMeta,
      uri: '/src/baz.ts',
    });
    expect(result.success).toBe(true);
    expect(result.data?.uri).toBe('/src/baz.ts');
  });
});

describe('lspFindReferences — filePath alias', () => {
  it('accepts filePath as alias for uri', () => {
    const result = LSPFindReferencesQuerySchema.safeParse({
      ...baseMeta,
      filePath: '/src/foo.ts',
    });
    expect(result.success).toBe(true);
    expect(result.data?.uri).toBe('/src/foo.ts');
  });

  it('rejects input with neither uri nor filePath', () => {
    const result = LSPFindReferencesQuerySchema.safeParse({ ...baseMeta });
    expect(result.success).toBe(false);
  });
});

describe('lspCallHierarchy — filePath alias', () => {
  it('accepts filePath as alias for uri', () => {
    const result = LSPCallHierarchyQuerySchema.safeParse({
      ...baseMeta,
      filePath: '/src/foo.ts',
      direction: 'incoming',
    });
    expect(result.success).toBe(true);
    expect(result.data?.uri).toBe('/src/foo.ts');
  });

  it('rejects input with neither uri nor filePath', () => {
    const result = LSPCallHierarchyQuerySchema.safeParse({
      ...baseMeta,
      direction: 'incoming',
    });
    expect(result.success).toBe(false);
  });
});
