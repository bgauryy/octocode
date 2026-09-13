import { describe, expect, it } from 'vitest';
import { scanSucceeded } from '../../../src/tools/ast_rewrite/astGrep.js';

describe('ast-grep empty scan classification', () => {
  const empty = { success: false, exitCode: 1, stdout: '[]\n', stderr: '' };

  it('recognizes the explicit no-match exit status', () => {
    expect(scanSucceeded(empty)).toBe(true);
    expect(scanSucceeded({ ...empty, stdout: '[ \n ]' })).toBe(true);
  });

  it.each([
    { stdout: '' },
    { stdout: 'invalid JSON' },
    { stderr: 'file could not be read' },
    { exitCode: 2 },
    { timedOut: true },
    { outputLimitExceeded: true },
    { error: new Error('spawn failed') },
  ])('does not hide scan failures: %j', failure => {
    expect(scanSucceeded({ ...empty, ...failure })).toBe(false);
  });
});
