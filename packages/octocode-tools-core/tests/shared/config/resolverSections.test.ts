import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveLocal,
} from '../../../src/shared/config/resolverSections.js';
import {
  setRuntimeSurface,
  _resetRuntimeSurface,
} from '../../../src/shared/config/runtimeSurface.js';

describe('parseBooleanEnv', () => {
  it('should return undefined for undefined', () => {
    expect(parseBooleanEnv(undefined)).toBeUndefined();
  });

  it('should return undefined for null', () => {
    expect(parseBooleanEnv(null as unknown as undefined)).toBeUndefined();
  });

  it('should return undefined for empty string', () => {
    expect(parseBooleanEnv('')).toBeUndefined();
  });

  it('should return undefined for whitespace-only', () => {
    expect(parseBooleanEnv('   ')).toBeUndefined();
  });

  it('should return true for "true"', () => {
    expect(parseBooleanEnv('true')).toBe(true);
  });

  it('should return true for "TRUE"', () => {
    expect(parseBooleanEnv('TRUE')).toBe(true);
  });

  it('should return true for "1"', () => {
    expect(parseBooleanEnv('1')).toBe(true);
  });

  it('should return false for "false"', () => {
    expect(parseBooleanEnv('false')).toBe(false);
  });

  it('should return false for "FALSE"', () => {
    expect(parseBooleanEnv('FALSE')).toBe(false);
  });

  it('should return false for "0"', () => {
    expect(parseBooleanEnv('0')).toBe(false);
  });

  it('should return undefined for unrecognized values', () => {
    expect(parseBooleanEnv('yes')).toBeUndefined();
    expect(parseBooleanEnv('no')).toBeUndefined();
    expect(parseBooleanEnv('anything')).toBeUndefined();
  });

  it('should trim whitespace before parsing', () => {
    expect(parseBooleanEnv('  true  ')).toBe(true);
    expect(parseBooleanEnv(' false ')).toBe(false);
    expect(parseBooleanEnv(' 1 ')).toBe(true);
    expect(parseBooleanEnv('\t0\t')).toBe(false);
  });
});

describe('parseIntEnv', () => {
  it('should return undefined for undefined', () => {
    expect(parseIntEnv(undefined)).toBeUndefined();
  });

  it('should return undefined for null', () => {
    expect(parseIntEnv(null as unknown as undefined)).toBeUndefined();
  });

  it('should return undefined for empty string', () => {
    expect(parseIntEnv('')).toBeUndefined();
  });

  it('should return undefined for whitespace-only', () => {
    expect(parseIntEnv('   ')).toBeUndefined();
  });

  it('should parse valid integers', () => {
    expect(parseIntEnv('42')).toBe(42);
    expect(parseIntEnv('0')).toBe(0);
    expect(parseIntEnv('-5')).toBe(-5);
    expect(parseIntEnv('30000')).toBe(30000);
  });

  it('should return undefined for non-numeric strings', () => {
    expect(parseIntEnv('not-a-number')).toBeUndefined();
    expect(parseIntEnv('abc')).toBeUndefined();
  });

  it('should trim whitespace before parsing', () => {
    expect(parseIntEnv('  42  ')).toBe(42);
    expect(parseIntEnv('\t100\t')).toBe(100);
  });

  it('should parse floats as integers (truncating)', () => {
    expect(parseIntEnv('3.14')).toBe(3);
    expect(parseIntEnv('99.9')).toBe(99);
  });
});

describe('parseStringArrayEnv', () => {
  it('should return undefined for undefined', () => {
    expect(parseStringArrayEnv(undefined)).toBeUndefined();
  });

  it('should return undefined for null', () => {
    expect(parseStringArrayEnv(null as unknown as undefined)).toBeUndefined();
  });

  it('should return undefined for empty string', () => {
    expect(parseStringArrayEnv('')).toBeUndefined();
  });

  it('should return undefined for whitespace-only', () => {
    expect(parseStringArrayEnv('   ')).toBeUndefined();
  });

  it('should parse comma-separated values', () => {
    expect(parseStringArrayEnv('a,b,c')).toEqual(['a', 'b', 'c']);
  });

  it('should trim whitespace from each value', () => {
    expect(parseStringArrayEnv(' a , b , c ')).toEqual(['a', 'b', 'c']);
  });

  it('should filter out empty entries', () => {
    expect(parseStringArrayEnv('a,,b, ,c')).toEqual(['a', 'b', 'c']);
  });

  it('should handle single value', () => {
    expect(parseStringArrayEnv('only')).toEqual(['only']);
  });
});

describe('resolveLocal', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env.ENABLE_LOCAL;
    delete process.env.ENABLE_CLONE;
    delete process.env.WORKSPACE_ROOT;
    delete process.env.ALLOWED_PATHS;
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  describe('CLI surface ignores ENABLE_LOCAL (local always enabled)', () => {
    afterEach(() => {
      _resetRuntimeSurface();
    });

    it('enables local on the CLI surface even when ENABLE_LOCAL=false', () => {
      setRuntimeSurface('cli');
      process.env.ENABLE_LOCAL = 'false';
      expect(resolveLocal(undefined).enabled).toBe(true);
    });

    it('enables local on the CLI surface even when file config disables it', () => {
      setRuntimeSurface('cli');
      expect(resolveLocal({ enabled: false }).enabled).toBe(true);
    });

    it('still honors ENABLE_LOCAL=false on the MCP surface (contrast)', () => {
      setRuntimeSurface('mcp');
      process.env.ENABLE_LOCAL = 'false';
      expect(resolveLocal(undefined).enabled).toBe(false);
    });
  });

  describe('enabled defaults', () => {
    it('should default enabled to false for the MCP surface when no config or env var', () => {
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(false);
    });

    it('should respect ENABLE_LOCAL=false override', () => {
      process.env.ENABLE_LOCAL = 'false';
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(false);
    });

    it('should respect ENABLE_LOCAL=true override', () => {
      process.env.ENABLE_LOCAL = 'true';
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(true);
    });

    it('should respect file config enabled=false override', () => {
      const result = resolveLocal({ enabled: false });
      expect(result.enabled).toBe(false);
    });
  });

  describe('enableClone defaults', () => {
    it('should default enableClone to false when no config or env var', () => {
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(false);
    });

    it('should default enableClone to false when file config has no enableClone', () => {
      const result = resolveLocal({ enabled: true });
      expect(result.enableClone).toBe(false);
    });
  });

  describe('enableClone from file config', () => {
    it('should use file config enableClone when env var is not set', () => {
      const result = resolveLocal({ enableClone: true });
      expect(result.enableClone).toBe(true);
    });

    it('should use file config enableClone=false', () => {
      const result = resolveLocal({ enableClone: false });
      expect(result.enableClone).toBe(false);
    });
  });

  describe('ENABLE_CLONE env var', () => {
    it('should enable clone when ENABLE_CLONE=true', () => {
      process.env.ENABLE_CLONE = 'true';
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(true);
    });

    it('should enable clone when ENABLE_CLONE=1', () => {
      process.env.ENABLE_CLONE = '1';
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(true);
    });

    it('should disable clone when ENABLE_CLONE=false', () => {
      process.env.ENABLE_CLONE = 'false';
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(false);
    });

    it('should disable clone when ENABLE_CLONE=0', () => {
      process.env.ENABLE_CLONE = '0';
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(false);
    });

    it('should fall back to default when ENABLE_CLONE is empty', () => {
      process.env.ENABLE_CLONE = '';
      const result = resolveLocal(undefined);
      expect(result.enableClone).toBe(false);
    });
  });

  describe('ENABLE_CLONE env var overrides file config', () => {
    it('ENABLE_CLONE=true overrides file enableClone: false', () => {
      process.env.ENABLE_CLONE = 'true';
      const result = resolveLocal({ enableClone: false });
      expect(result.enableClone).toBe(true);
    });

    it('ENABLE_CLONE=false overrides file enableClone: true', () => {
      process.env.ENABLE_CLONE = 'false';
      const result = resolveLocal({ enableClone: true });
      expect(result.enableClone).toBe(false);
    });
  });

  describe('enableClone is independent from enabled', () => {
    it('enableClone can be true while enabled is false', () => {
      const result = resolveLocal({ enabled: false, enableClone: true });
      expect(result.enabled).toBe(false);
      expect(result.enableClone).toBe(true);
    });

    it('enableClone can be false while enabled is true', () => {
      const result = resolveLocal({ enabled: true, enableClone: false });
      expect(result.enabled).toBe(true);
      expect(result.enableClone).toBe(false);
    });

    it('both can be true', () => {
      process.env.ENABLE_LOCAL = 'true';
      process.env.ENABLE_CLONE = 'true';
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(true);
      expect(result.enableClone).toBe(true);
    });
  });
});
