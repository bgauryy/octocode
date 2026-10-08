import { describe, it, expect } from 'vitest';
import { EXIT } from '../../src/cli/exit-codes.js';

describe('exit codes', () => {
  it('matches the native exit table for the codes Node emits', () => {
    expect(EXIT).toEqual({ OK: 0, GENERAL: 1, USAGE: 2, TOOL: 5 });
  });
});
