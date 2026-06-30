import type { SensitiveDataPattern } from './types.js';
import { normalizeCommandName } from './commandUtils.js';

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
  // Reset lastIndex so a global/sticky regex doesn't skip the test input.
  // Note: this is a timing heuristic, not a structural guarantee — it catches
  // obvious exponential backtracking patterns but may miss subtler ones.
  regex.lastIndex = 0;
  const start = performance.now();
  try {
    regex.test(REDOS_TEST_INPUT);
  } catch {
    return false;
  } finally {
    regex.lastIndex = 0;
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

  get version(): number {
    return this._version;
  }

  get extraSecretPatterns(): readonly SensitiveDataPattern[] {
    return (this._frozenSecretPatterns ??= Object.freeze([
      ...this._extraSecretPatterns,
    ]));
  }

  get extraAllowedCommands(): readonly string[] {
    return (this._frozenAllowedCommands ??= Object.freeze([
      ...this._extraAllowedCommands,
    ]));
  }

  get extraAllowedRoots(): readonly string[] {
    return (this._frozenAllowedRoots ??= Object.freeze([
      ...this._extraAllowedRoots,
    ]));
  }

  get extraIgnoredPathPatterns(): readonly RegExp[] {
    return (this._frozenIgnoredPathPatterns ??= Object.freeze([
      ...this._extraIgnoredPathPatterns,
    ]));
  }

  get extraIgnoredFilePatterns(): readonly RegExp[] {
    return (this._frozenIgnoredFilePatterns ??= Object.freeze([
      ...this._extraIgnoredFilePatterns,
    ]));
  }

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

  addAllowedCommands(commands: string[]): void {
    this._assertMutable();
    for (const cmd of commands) {
      if (typeof cmd !== 'string' || cmd.trim() === '') {
        throw new Error('Each command must be a non-empty string');
      }
      const normalized = normalizeCommandName(cmd);
      if (!this._extraAllowedCommands.includes(normalized)) {
        this._extraAllowedCommands.push(normalized);
      }
    }
    this._invalidateFrozenCaches();
    this._version++;
  }

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

  freeze(): void {
    this._frozen = true;
  }

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

export const securityRegistry: SecurityRegistry =
  ((globalThis as Record<string, unknown>)[GLOBAL_KEY] as SecurityRegistry) ??
  ((globalThis as Record<string, unknown>)[GLOBAL_KEY] =
    new SecurityRegistry());
