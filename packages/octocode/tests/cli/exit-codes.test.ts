import { describe, it, expect } from 'vitest';
import { EXIT } from '../../src/cli/exit-codes.js';

describe('exit codes', () => {
  it('defines the typed contract', () => {
    expect(EXIT.OK).toBe(0);
    expect(EXIT.GENERAL).toBe(1);
    expect(EXIT.USAGE).toBe(2);
    expect(EXIT.NOT_FOUND).toBe(3);
    expect(EXIT.AUTH).toBe(4);
    expect(EXIT.TOOL).toBe(5);
    expect(EXIT.RATE_LIMIT).toBe(7);
  });
});
