import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { SecurityRegistry, securityRegistry } from '../../src/security/registry.js';

// Each test uses a FRESH SecurityRegistry instance to avoid cross-test
// pollution via the shared global singleton.

describe('SecurityRegistry', () => {
  let reg: SecurityRegistry;

  beforeEach(() => {
    reg = new SecurityRegistry();
  });

  afterEach(() => {
    // Guarantee: reset the global singleton if any test touched it.
    securityRegistry.reset();
  });

  // ── addSecretPatterns ─────────────────────────────────────────────────────

  it('adds secret patterns and increments version', () => {
    expect(reg.version).toBe(0);
    reg.addSecretPatterns([{ name: 'test-token', description: 'test-token test pattern', regex: /TEST-[a-z]+/g }]);
    expect(reg.extraSecretPatterns).toHaveLength(1);
    expect(reg.version).toBe(1);
  });

  it('deduplicates secret patterns by name', () => {
    reg.addSecretPatterns([{ name: 'dup', description: 'dup test pattern', regex: /dup-[a-z]+/g }]);
    reg.addSecretPatterns([{ name: 'dup', description: 'dup test pattern', regex: /dup-[a-z]+/g }]);
    expect(reg.extraSecretPatterns).toHaveLength(1);
  });

  // Note: the registry's ReDoS check is a timing heuristic (50ms wall-clock).
  // Modern V8 JIT optimizes many classic ReDoS patterns to near-zero, making
  // timing assertions inherently flaky. We verify the structural contract
  // (valid safe patterns are accepted) but do not assert on the heuristic path.

  // ── addAllowedCommands ────────────────────────────────────────────────────

  it('adds allowed commands and increments version', () => {
    reg.addAllowedCommands(['git', 'npm']);
    expect(reg.extraAllowedCommands).toContain('git');
    expect(reg.version).toBe(1);
  });

  it('deduplicates allowed commands', () => {
    reg.addAllowedCommands(['git']);
    reg.addAllowedCommands(['git']);
    expect(reg.extraAllowedCommands).toHaveLength(1);
  });

  it('rejects empty command strings', () => {
    expect(() => reg.addAllowedCommands([''])).toThrow();
  });

  // ── addAllowedRoots ───────────────────────────────────────────────────────

  it('adds allowed roots', () => {
    reg.addAllowedRoots(['/home/user/project']);
    expect(reg.extraAllowedRoots).toContain('/home/user/project');
    expect(reg.version).toBe(1);
  });

  it('rejects empty root strings', () => {
    expect(() => reg.addAllowedRoots([''])).toThrow();
  });

  // ── addIgnoredPathPatterns ────────────────────────────────────────────────

  it('adds ignored path patterns', () => {
    reg.addIgnoredPathPatterns([/node_modules/]);
    expect(reg.extraIgnoredPathPatterns).toHaveLength(1);
  });



  // ── addIgnoredFilePatterns ────────────────────────────────────────────────

  it('adds ignored file patterns', () => {
    reg.addIgnoredFilePatterns([/\.lock$/]);
    expect(reg.extraIgnoredFilePatterns).toHaveLength(1);
  });



  // ── freeze / frozen guard ─────────────────────────────────────────────────

  it('freeze() prevents further mutations', () => {
    reg.freeze();
    expect(reg.frozen).toBe(true);
    expect(() => reg.addSecretPatterns([{ name: 'x', description: 'x test pattern', regex: /x/g }])).toThrow();
    expect(() => reg.addAllowedCommands(['echo'])).toThrow();
    expect(() => reg.addAllowedRoots(['/tmp'])).toThrow();
    expect(() => reg.addIgnoredPathPatterns([/tmp/])).toThrow();
    expect(() => reg.addIgnoredFilePatterns([/tmp/])).toThrow();
  });

  it('reset() after freeze() re-enables mutation', () => {
    reg.freeze();
    reg.reset();
    expect(reg.frozen).toBe(false);
    expect(() => reg.addSecretPatterns([{ name: 'y', description: 'y test pattern', regex: /y/g }])).not.toThrow();
  });

  // ── reset ─────────────────────────────────────────────────────────────────

  it('reset() clears all collections, unfreezes, and bumps version', () => {
    reg.addSecretPatterns([{ name: 'tok', description: 'tok test pattern', regex: /tok-[a-z]+/g }]);
    reg.addAllowedCommands(['ls']);
    reg.addAllowedRoots(['/home']);
    reg.freeze();
    const vBefore = reg.version;
    reg.reset();
    expect(reg.extraSecretPatterns).toHaveLength(0);
    expect(reg.extraAllowedCommands).toHaveLength(0);
    expect(reg.extraAllowedRoots).toHaveLength(0);
    expect(reg.frozen).toBe(false);
    expect(reg.version).toBeGreaterThan(vBefore);
  });

  // ── global singleton ──────────────────────────────────────────────────────

  it('securityRegistry singleton exposes the ISecurityRegistry contract', () => {
    expect(typeof securityRegistry.addSecretPatterns).toBe('function');
    expect(typeof securityRegistry.freeze).toBe('function');
    expect(typeof securityRegistry.reset).toBe('function');
    expect(typeof securityRegistry.version).toBe('number');
    expect(typeof securityRegistry.frozen).toBe('boolean');
  });
});
