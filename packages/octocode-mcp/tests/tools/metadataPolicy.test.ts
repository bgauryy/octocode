import { describe, expect, it, vi } from 'vitest';
import { z } from 'zod';
import { hasValidMetadata } from '../../src/tools/metadataPolicy.js';
import { TOOL_METADATA_ERRORS } from '../../../octocode-tools-core/src/errors/domainErrors.js';
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
    const logError = vi.fn();
    const valid = hasValidMetadata(
      makeTool({ name: 'x', skipMetadataCheck: true }),
      {
        hasTool,
        logSessionErrorSafe: logError,
      }
    );

    expect(valid).toBe(true);
    expect(hasTool).not.toHaveBeenCalled();
    expect(logError).not.toHaveBeenCalled();
  });

  it('returns false and logs INVALID_FORMAT when missing from metadata', () => {
    const hasTool = vi.fn().mockReturnValue(false);
    const logError = vi.fn();
    const valid = hasValidMetadata(makeTool({ name: 'x' }), {
      hasTool,
      logSessionErrorSafe: logError,
    });

    expect(valid).toBe(false);
    expect(logError).toHaveBeenCalledWith(
      'x',
      TOOL_METADATA_ERRORS.INVALID_FORMAT.code
    );
  });

  it('returns false and logs INVALID_API_RESPONSE when metadata lookup throws', () => {
    const hasTool = vi.fn().mockImplementation(() => {
      throw new Error('boom');
    });
    const logError = vi.fn();
    const valid = hasValidMetadata(makeTool({ name: 'x' }), {
      hasTool,
      logSessionErrorSafe: logError,
    });

    expect(valid).toBe(false);
    expect(logError).toHaveBeenCalledWith(
      'x',
      TOOL_METADATA_ERRORS.INVALID_API_RESPONSE.code
    );
  });
});
