import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  parseLoggingEnv,
  resolveTelemetry,
  resolveLocal,
} from '../../src/config/resolverSections.js';

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

describe('parseLoggingEnv', () => {
  it('should return undefined for undefined input', () => {
    expect(parseLoggingEnv(undefined)).toBeUndefined();
  });

  it('should return undefined for empty string', () => {
    expect(parseLoggingEnv('')).toBeUndefined();
  });

  it('should return undefined for whitespace-only string', () => {
    expect(parseLoggingEnv('   ')).toBeUndefined();
  });

  it('should return false for "false"', () => {
    expect(parseLoggingEnv('false')).toBe(false);
  });

  it('should return false for "FALSE"', () => {
    expect(parseLoggingEnv('FALSE')).toBe(false);
  });

  it('should return false for "False"', () => {
    expect(parseLoggingEnv('False')).toBe(false);
  });

  it('should return false for "0"', () => {
    expect(parseLoggingEnv('0')).toBe(false);
  });

  it('should return false for " false " with whitespace', () => {
    expect(parseLoggingEnv(' false ')).toBe(false);
  });

  it('should return false for " 0 " with whitespace', () => {
    expect(parseLoggingEnv(' 0 ')).toBe(false);
  });

  it('should return true for "true"', () => {
    expect(parseLoggingEnv('true')).toBe(true);
  });

  it('should return true for "TRUE"', () => {
    expect(parseLoggingEnv('TRUE')).toBe(true);
  });

  it('should return true for "1"', () => {
    expect(parseLoggingEnv('1')).toBe(true);
  });

  it('should return true for "yes"', () => {
    expect(parseLoggingEnv('yes')).toBe(true);
  });

  it('should return true for "enabled"', () => {
    expect(parseLoggingEnv('enabled')).toBe(true);
  });

  it('should return true for any unrecognized non-false value', () => {
    expect(parseLoggingEnv('anything')).toBe(true);
    expect(parseLoggingEnv('random')).toBe(true);
    expect(parseLoggingEnv('on')).toBe(true);
  });

  it('should return true for " true " with whitespace', () => {
    expect(parseLoggingEnv(' true ')).toBe(true);
  });

  it('should return true for " yes " with whitespace', () => {
    expect(parseLoggingEnv(' yes ')).toBe(true);
  });

  describe('key difference from parseBooleanEnv', () => {
    it('parseBooleanEnv returns undefined for "yes", parseLoggingEnv returns true', () => {
      expect(parseBooleanEnv('yes')).toBeUndefined();
      expect(parseLoggingEnv('yes')).toBe(true);
    });

    it('parseBooleanEnv returns undefined for "enabled", parseLoggingEnv returns true', () => {
      expect(parseBooleanEnv('enabled')).toBeUndefined();
      expect(parseLoggingEnv('enabled')).toBe(true);
    });

    it('both agree on "true"/"false"/"0"/"1"', () => {
      expect(parseBooleanEnv('true')).toBe(parseLoggingEnv('true'));
      expect(parseBooleanEnv('false')).toBe(parseLoggingEnv('false'));
      expect(parseBooleanEnv('0')).toBe(parseLoggingEnv('0'));
      expect(parseBooleanEnv('1')).toBe(parseLoggingEnv('1'));
    });
  });
});

describe('resolveTelemetry', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env.LOG;
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it('should return default (true) when LOG not set and no file config', () => {
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(true);
  });

  it('should use file config when LOG not set', () => {
    const result = resolveTelemetry({ logging: false });
    expect(result.logging).toBe(false);
  });

  it('should return false when LOG=false', () => {
    process.env.LOG = 'false';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(false);
  });

  it('should return false when LOG=0', () => {
    process.env.LOG = '0';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(false);
  });

  it('should return true when LOG=true', () => {
    process.env.LOG = 'true';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(true);
  });

  it('should return true when LOG=yes (default-to-true semantics)', () => {
    process.env.LOG = 'yes';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(true);
  });

  it('should return true when LOG=anything (default-to-true semantics)', () => {
    process.env.LOG = 'anything';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(true);
  });

  it('should use default when LOG is empty string', () => {
    process.env.LOG = '';
    const result = resolveTelemetry(undefined);
    expect(result.logging).toBe(true);
  });

  describe('env overrides file config', () => {
    it('LOG=false overrides file logging: true', () => {
      process.env.LOG = 'false';
      const result = resolveTelemetry({ logging: true });
      expect(result.logging).toBe(false);
    });

    it('LOG=yes overrides file logging: false', () => {
      process.env.LOG = 'yes';
      const result = resolveTelemetry({ logging: false });
      expect(result.logging).toBe(true);
    });

    it('LOG=anything overrides file logging: false', () => {
      process.env.LOG = 'enabled';
      const result = resolveTelemetry({ logging: false });
      expect(result.logging).toBe(true);
    });

    it('LOG=true overrides file logging: false', () => {
      process.env.LOG = 'true';
      const result = resolveTelemetry({ logging: false });
      expect(result.logging).toBe(true);
    });
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

  describe('enabled defaults', () => {
    it('should default enabled to true when no config or env var', () => {
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(true);
    });

    it('should respect ENABLE_LOCAL=false override', () => {
      process.env.ENABLE_LOCAL = 'false';
      const result = resolveLocal(undefined);
      expect(result.enabled).toBe(false);
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
