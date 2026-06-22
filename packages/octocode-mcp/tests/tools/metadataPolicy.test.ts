import { describe, expect, it, vi } from 'vitest';
import { z } from 'zod';
import { hasValidMetadata } from '../../src/tools/metadataPolicy.js';
import type { ToolConfig } from '../../src/tools/toolConfig.js';

function makeTool(
  overrides: Partial<ToolConfig> & Pick<ToolConfig, 'name'>
): ToolConfig {
  return {
    name: overrides.name,
    description: '',
    isDefault: overrides.isDefault ?? true,
    isLocal: overrides.isLocal ?? false,
    isClone: overrides.isClone,
    type: overrides.type ?? 'search',
    skipMetadataCheck: overrides.skipMetadataCheck,
    fn: overrides.fn ?? (() => ({}) as never),
    direct: overrides.direct ?? {
      schema: z.object({}),
      inputSchema: z.object({}),
      executionFn: async () => ({ content: [] }),
      security: 'basic',
    },
  };
}

describe('metadataPolicy', () => {
  it('skips metadata check when skipMetadataCheck=true', () => {
    const hasTool = vi.fn();
    const valid = hasValidMetadata(
      makeTool({ name: 'x', skipMetadataCheck: true }),
      { hasTool }
    );

    expect(valid).toBe(true);
    expect(hasTool).not.toHaveBeenCalled();
  });

  it('returns false when missing from metadata', () => {
    const hasTool = vi.fn().mockReturnValue(false);
    const valid = hasValidMetadata(makeTool({ name: 'x' }), {
      hasTool,
    });

    expect(valid).toBe(false);
  });

  it('returns false when metadata lookup throws', () => {
    const hasTool = vi.fn().mockImplementation(() => {
      throw new Error('boom');
    });
    const valid = hasValidMetadata(makeTool({ name: 'x' }), {
      hasTool,
    });

    expect(valid).toBe(false);
  });
});
