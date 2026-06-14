import { describe, it, expect } from 'vitest';
import { validateCommand } from 'octocode-security/commandValidator';
import { resolveRipgrepBinary } from '../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js';

describe('bundled @vscode/ripgrep — absolute-path validator wiring', () => {
  it('resolveRipgrepBinary() returns a path the security validator accepts', () => {
    const binary = resolveRipgrepBinary();

    expect(typeof binary).toBe('string');
    expect(binary.length).toBeGreaterThan(0);

    const validation = validateCommand(binary, [
      '-n',
      '--column',
      '-S',
      '--color',
      'never',
      '--sort',
      'path',
      '--',
      'pattern',
      '/some/path',
    ]);

    expect(
      validation,
      `validator rejected resolveRipgrepBinary() output (${binary}) — bundled rg path would fail in production`
    ).toEqual({ isValid: true });
  });

  it('still applies per-command rules to the resolved binary (rejects unknown flags)', () => {
    const binary = resolveRipgrepBinary();
    const validation = validateCommand(binary, [
      '--definitely-not-a-real-flag',
      'pattern',
      '/some/path',
    ]);

    expect(validation.isValid).toBe(false);
  });
});
