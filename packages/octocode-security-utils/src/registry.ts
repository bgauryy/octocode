/**
 * SecurityRegistry — Central extensibility point for all security APIs.
 *
 * Users can add custom:
 *   - Secret detection patterns (regex)
 *   - Allowed commands
 *   - Ignored path/file patterns
 *
 * All security APIs read from the registry at call time, so extensions
 * take effect immediately after registration.
 *
 * @example
 * ```ts
 * import { securityRegistry } from 'octocode-security-utils';
 *
 * // Add a custom secret pattern
 * securityRegistry.addSecretPatterns([{
 *   name: 'myInternalToken',
 *   description: 'Internal service token',
 *   regex: /\bMYCORP_[A-Z0-9]{32}\b/g,
 *   matchAccuracy: 'high',
 * }]);
 *
 * // Add a custom allowed command
 * securityRegistry.addAllowedCommands(['jq', 'yq']);
 *
 * // Add a custom ignored path
 * securityRegistry.addIgnoredPathPatterns([/^\.vault$/]);
 *
 * // Add a custom ignored file
 * securityRegistry.addIgnoredFilePatterns([/^internal[-_]secrets\.ya?ml$/]);
 * ```
 */

import type { SensitiveDataPattern } from './types.js';

/** Abstraction for the security registry (Dependency Inversion). */
export interface ISecurityRegistry {
  readonly extraSecretPatterns: readonly SensitiveDataPattern[];
  readonly extraAllowedCommands: readonly string[];
  readonly extraAllowedRoots: readonly string[];
  readonly extraIgnoredPathPatterns: readonly RegExp[];
  readonly extraIgnoredFilePatterns: readonly RegExp[];
  readonly version: number;
  readonly frozen: boolean;
  addSecretPatterns(patterns: SensitiveDataPattern[]): void;
  addAllowedCommands(commands: string[]): void;
  addAllowedRoots(roots: string[]): void;
  addIgnoredPathPatterns(patterns: RegExp[]): void;
  addIgnoredFilePatterns(patterns: RegExp[]): void;
  freeze(): void;
  reset(): void;
}

const REDOS_TIMEOUT_MS = 50;
const REDOS_TEST_INPUT = 'a'.repeat(100);

function isReDoSSafe(regex: RegExp): boolean {
  const start = performance.now();
  try {
    regex.test(REDOS_TEST_INPUT);
  } catch {
    return false;
  }
  return performance.now() - start < REDOS_TIMEOUT_MS;
}

export class SecurityRegistry implements ISecurityRegistry {
  private _extraSecretPatterns: SensitiveDataPattern[] = [];
  private _extraAllowedCommands: string[] = [];
  private _extraAllowedRoots: string[] = [];
  private _extraIgnoredPathPatterns: RegExp[] = [];
  private _extraIgnoredFilePatterns: RegExp[] = [];
  private _version = 0;
  private _frozen = false;

  private _frozenSecretPatterns: readonly SensitiveDataPattern[] | null = null;
  private _frozenAllowedCommands: readonly string[] | null = null;
  private _frozenAllowedRoots: readonly string[] | null = null;
  private _frozenIgnoredPathPatterns: readonly RegExp[] | null = null;
  private _frozenIgnoredFilePatterns: readonly RegExp[] | null = null;

  /** Whether the registry is locked against further mutations. */
  get frozen(): boolean {
    return this._frozen;
  }

  private _assertMutable(): void {
    if (this._frozen) {
      throw new Error(
        'SecurityRegistry is frozen — call reset() to unfreeze before mutating'
      );
    }
  }

  private _invalidateFrozenCaches(): void {
    this._frozenSecretPatterns = null;
    this._frozenAllowedCommands = null;
    this._frozenAllowedRoots = null;
    this._frozenIgnoredPathPatterns = null;
    this._frozenIgnoredFilePatterns = null;
  }

  /** Monotonic counter incremented on every mutation. Used for cache invalidation. */
  get version(): number {
    return this._version;
  }

  /** User-registered secret detection patterns (frozen, cached between mutations). */
  get extraSecretPatterns(): readonly SensitiveDataPattern[] {
    return (this._frozenSecretPatterns ??= Object.freeze([
      ...this._extraSecretPatterns,
    ]));
  }

  /** User-registered allowed commands (frozen, cached between mutations). */
  get extraAllowedCommands(): readonly string[] {
    return (this._frozenAllowedCommands ??= Object.freeze([
      ...this._extraAllowedCommands,
    ]));
  }

  /** User-registered additional root directories (frozen, cached between mutations). */
  get extraAllowedRoots(): readonly string[] {
    return (this._frozenAllowedRoots ??= Object.freeze([
      ...this._extraAllowedRoots,
    ]));
  }

  /** User-registered ignored path patterns (frozen, cached between mutations). */
  get extraIgnoredPathPatterns(): readonly RegExp[] {
    return (this._frozenIgnoredPathPatterns ??= Object.freeze([
      ...this._extraIgnoredPathPatterns,
    ]));
  }

  /** User-registered ignored file patterns (frozen, cached between mutations). */
  get extraIgnoredFilePatterns(): readonly RegExp[] {
    return (this._frozenIgnoredFilePatterns ??= Object.freeze([
      ...this._extraIgnoredFilePatterns,
    ]));
  }

  /**
   * Register additional secret detection patterns.
   * Deduplicates by pattern name — duplicate names are silently skipped.
   */
  addSecretPatterns(patterns: SensitiveDataPattern[]): void {
    this._assertMutable();
    for (const p of patterns) {
      if (!p.name || !p.regex) {
        throw new Error('Each pattern must have a name and regex');
      }
      if (!isReDoSSafe(p.regex)) {
        throw new Error(
          `Pattern '${p.name}' failed ReDoS safety check — regex may cause catastrophic backtracking`
        );
      }
      if (!this._extraSecretPatterns.some(e => e.name === p.name)) {
        this._extraSecretPatterns.push(p);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

  /**
   * Register additional allowed commands for the command validator.
   * These are merged with the built-in allowed commands at call time.
   */
  addAllowedCommands(commands: string[]): void {
    this._assertMutable();
    for (const cmd of commands) {
      if (typeof cmd !== 'string' || cmd.trim() === '') {
        throw new Error('Each command must be a non-empty string');
      }
      if (!this._extraAllowedCommands.includes(cmd)) {
        this._extraAllowedCommands.push(cmd);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

  /**
   * Register additional root directories for path and execution context validation.
   * Use this to allow access to app-specific directories (e.g. cloned repos folder).
   */
  addAllowedRoots(roots: string[]): void {
    this._assertMutable();
    for (const root of roots) {
      if (typeof root !== 'string' || root.trim() === '') {
        throw new Error('Each root must be a non-empty string');
      }
      if (!this._extraAllowedRoots.includes(root)) {
        this._extraAllowedRoots.push(root);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

  /**
   * Register additional ignored path patterns.
   * Deduplicates by regex source — duplicate sources are silently skipped.
   */
  addIgnoredPathPatterns(patterns: RegExp[]): void {
    this._assertMutable();
    for (const p of patterns) {
      if (!isReDoSSafe(p)) {
        throw new Error(`Path pattern /${p.source}/ failed ReDoS safety check`);
      }
      if (!this._extraIgnoredPathPatterns.some(e => e.source === p.source)) {
        this._extraIgnoredPathPatterns.push(p);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

  /**
   * Register additional ignored file patterns.
   * Deduplicates by regex source — duplicate sources are silently skipped.
   */
  addIgnoredFilePatterns(patterns: RegExp[]): void {
    this._assertMutable();
    for (const p of patterns) {
      if (!isReDoSSafe(p)) {
        throw new Error(`File pattern /${p.source}/ failed ReDoS safety check`);
      }
      if (!this._extraIgnoredFilePatterns.some(e => e.source === p.source)) {
        this._extraIgnoredFilePatterns.push(p);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

  /**
   * Lock the registry, preventing further mutations.
   * Call after all startup configuration is complete to harden at runtime.
   * Use `reset()` to unfreeze and clear all extensions.
   *
   * @example
   * ```ts
   * securityRegistry.addAllowedCommands(['jq']);
   * securityRegistry.freeze();
   * // All further add* calls will throw
   * ```
   */
  freeze(): void {
    this._frozen = true;
  }

  /** Remove all user-registered extensions and unfreeze. Useful for testing. */
  reset(): void {
    this._frozen = false;
    this._extraSecretPatterns = [];
    this._extraAllowedCommands = [];
    this._extraAllowedRoots = [];
    this._extraIgnoredPathPatterns = [];
    this._extraIgnoredFilePatterns = [];
    this._invalidateFrozenCaches();
    this._version++;
  }
}

const GLOBAL_KEY = '__octocode_security_registry__';

/**
 * Global singleton registry. Uses globalThis to survive module duplication
 * (e.g. vitest transforms, dual ESM/CJS loading, or bundler code-splitting).
 */
export const securityRegistry: SecurityRegistry =
  ((globalThis as Record<string, unknown>)[GLOBAL_KEY] as SecurityRegistry) ??
  ((globalThis as Record<string, unknown>)[GLOBAL_KEY] =
    new SecurityRegistry());
