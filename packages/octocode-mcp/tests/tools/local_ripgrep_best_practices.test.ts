import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  preflightValidateRipgrepPattern,
  type RipgrepPatternValidation,
} from '../../src/tools/local_ripgrep/patternValidation.js';

describe('T1.6 — Ripgrep regex is validated pre-launch (cheap fail-fast)', () => {
  it('accepts a plain literal pattern (smartCase mode)', () => {
    const v: RipgrepPatternValidation = preflightValidateRipgrepPattern({
      pattern: 'foo',
    });
    expect(v.isValid).toBe(true);
  });

  it('flags an unmatched paren as invalid', () => {
    const v = preflightValidateRipgrepPattern({ pattern: '(foo' });
    expect(v.isValid).toBe(false);
    expect(v.errors.join(' ').toLowerCase()).toContain('regex');
  });

  it('flags a dangling escape as invalid', () => {
    const v = preflightValidateRipgrepPattern({ pattern: 'foo\\' });
    expect(v.isValid).toBe(false);
  });

  it('warns when pattern looks literal but fixedString is not set', () => {
    const v = preflightValidateRipgrepPattern({ pattern: 'console.log' });
    expect(v.isValid).toBe(true);
    expect(v.warnings.join(' ')).toMatch(/fixedString/i);
  });

  it('does NOT warn when fixedString is already true', () => {
    const v = preflightValidateRipgrepPattern({
      pattern: 'console.log',
      fixedString: true,
    });
    expect(v.warnings.join(' ')).not.toMatch(/fixedString/i);
  });

  it('warns on lookaround constructs without -P (server cost guidance)', () => {
    const v = preflightValidateRipgrepPattern({ pattern: 'foo(?=bar)' });
    expect(v.warnings.join(' ').toLowerCase()).toMatch(/lookaround|perlregex/);
  });

  it('skips literal-likeness warning for clearly regex patterns', () => {
    const v = preflightValidateRipgrepPattern({ pattern: '^foo$' });
    expect(v.warnings.join(' ')).not.toMatch(/fixedString/i);
  });

  it('accepts empty pattern with explicit error (so caller can short-circuit)', () => {
    const v = preflightValidateRipgrepPattern({ pattern: '' });
    expect(v.isValid).toBe(false);
  });
});

describe('T1.7 — fs.readdir pre-flight is removed from the ripgrep hot path', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('does NOT call fs.readdir or fs.stat during a normal ripgrep search', async () => {
    const readdirSpy = vi.fn(async () => []);
    const statSpy = vi.fn(async () => ({ size: 0 }));

    vi.doMock('fs', async () => {
      const real = await vi.importActual<typeof import('fs')>('fs');
      return {
        ...real,
        promises: {
          ...real.promises,
          readdir: readdirSpy,
          stat: statSpy,
        },
      };
    });

    vi.doMock('../../src/utils/exec/safe.js', () => ({
      safeExec: vi.fn(async () => ({
        success: true,
        stdout: '',
        stderr: '',
        code: 1,
      })),
    }));

    vi.doMock('../../src/utils/file/toolHelpers.js', async () => {
      const real = await vi.importActual<
        typeof import('../../src/utils/file/toolHelpers.js')
      >('../../src/utils/file/toolHelpers.js');
      return {
        ...real,
        validateToolPath: vi.fn(() => ({
          isValid: true,
          sanitizedPath: '/tmp/repo',
        })),
      };
    });

    vi.doMock('../../src/utils/exec/commandAvailability.js', () => ({
      checkCommandAvailability: vi.fn(async () => ({ available: true })),
      getMissingCommandError: vi.fn(() => ''),
    }));

    const { searchContentRipgrep } =
      await import('../../src/tools/local_ripgrep/searchContentRipgrep.js');

    await searchContentRipgrep({
      id: 'q1',
      researchGoal: 'test',
      reasoning: 'unit',
      pattern: 'foo',
      path: '/tmp/repo',
    } as Parameters<typeof searchContentRipgrep>[0]);

    expect(readdirSpy).not.toHaveBeenCalled();
    expect(statSpy).not.toHaveBeenCalled();
  });
});
