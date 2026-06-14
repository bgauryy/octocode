import { describe, it, expect } from 'vitest';
import { TOOL_NAMES } from '@octocodeai/octocode-tools-core';
import { STATIC_TOOL_NAMES } from '@octocodeai/octocode-tools-core';
import { HINTS } from '@octocodeai/octocode-tools-core';

describe('Debug proxy', () => {
  it('should show values', () => {
    expect(TOOL_NAMES.LOCAL_RIPGREP).toBe(STATIC_TOOL_NAMES.LOCAL_RIPGREP);
    expect(HINTS[STATIC_TOOL_NAMES.LOCAL_RIPGREP]).toBeDefined();
  });
});
