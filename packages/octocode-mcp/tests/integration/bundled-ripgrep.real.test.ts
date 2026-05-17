/**
 * Regression guard for the bundled-ripgrep absolute-path wiring.
 *
 * `resolveRipgrepBinary()` returns an absolute filesystem path to the
 * `@vscode/ripgrep` binary (e.g. `/.../node_modules/@vscode/ripgrep-darwin-arm64/bin/rg`).
 * `safeExec` then hands that string to `validateCommand` in
 * `octocode-security-utils`. Before the May-2026 fix, `validateCommand`
 * only checked literal command names (`'rg'`, `'find'`, etc.) and rejected
 * the absolute path with:
 *
 *   "Command '/.../bin/rg' is not allowed. Allowed commands: rg, ls, find, grep, git"
 *
 * Unit tests passed because they mock `safeExec`. The MCP server failed in
 * real use on every call. This test re-runs the wiring without mocks so a
 * future regression breaks fast and visibly.
 */
import { describe, it, expect } from 'vitest';
import { validateCommand } from 'octocode-security-utils/commandValidator';
import { resolveRipgrepBinary } from '../../src/utils/exec/ripgrepBinary.js';

describe('bundled @vscode/ripgrep — absolute-path validator wiring', () => {
  it('resolveRipgrepBinary() returns a path the security validator accepts', () => {
    const binary = resolveRipgrepBinary();

    expect(typeof binary).toBe('string');
    expect(binary.length).toBeGreaterThan(0);

    // Args mirror what RipgrepCommandBuilder.simple() emits so this test
    // tracks the real call shape.
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
