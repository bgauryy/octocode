import { describe, expect, it } from 'vitest';

import { detectPlatformId, executableNames } from '../../src/lsp/platform.js';

describe('platform', () => {
  it('detects a canonical {os}-{arch}[-musl] platform id', () => {
    const id = detectPlatformId();
    expect(id).toMatch(/^(darwin|linux|win32)-(x64|arm64)(-musl)?$/);
    // musl qualifier only ever appears on linux
    if (id.endsWith('-musl')) expect(id.startsWith('linux-')).toBe(true);
  });

  it('expands executable names by PATHEXT only on Windows', () => {
    const names = executableNames('gopls');
    expect(names[0]).toBe('gopls');
    if (process.platform === 'win32') {
      expect(names.length).toBeGreaterThan(1);
      expect(names.some(n => n.toLowerCase().endsWith('.exe'))).toBe(true);
    } else {
      expect(names).toEqual(['gopls']);
    }
  });
});
