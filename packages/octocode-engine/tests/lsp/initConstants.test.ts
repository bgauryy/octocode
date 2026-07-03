import { describe, expect, it } from 'vitest';
import {
  CLIENT_NAME,
  CLIENT_VERSION,
  TSSERVER_DEFAULT_OPTIONS,
  TSSERVER_LANGUAGE_IDS,
} from '../../src/lsp/initConstants.js';

describe('initConstants', () => {
  it('CLIENT_NAME is the expected package name', () => {
    expect(CLIENT_NAME).toBe('octocode-engine');
  });

  it('CLIENT_VERSION is a non-empty string (semver or dev fallback)', () => {
    expect(typeof CLIENT_VERSION).toBe('string');
    expect(CLIENT_VERSION.length).toBeGreaterThan(0);
    // Either a semver-ish version or the dev fallback.
    expect(CLIENT_VERSION).toMatch(/^\d+\.\d+\.\d+.*$|^0\.0\.0-dev$/);
  });

  it('TSSERVER_LANGUAGE_IDS is a Set containing core TS/JS language ids', () => {
    expect(TSSERVER_LANGUAGE_IDS).toBeInstanceOf(Set);
    expect(TSSERVER_LANGUAGE_IDS.has('typescript')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('typescriptreact')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('javascript')).toBe(true);
    expect(TSSERVER_LANGUAGE_IDS.has('javascriptreact')).toBe(true);
  });

  it('TSSERVER_DEFAULT_OPTIONS has the expected shape', () => {
    expect(TSSERVER_DEFAULT_OPTIONS).toHaveProperty('tsserver');
    expect(TSSERVER_DEFAULT_OPTIONS).toHaveProperty('preferences');
    const ts = TSSERVER_DEFAULT_OPTIONS['tsserver'] as Record<string, unknown>;
    expect(typeof ts['maxTsServerMemory']).toBe('number');
    expect(ts['maxTsServerMemory']).toBeGreaterThan(0);
  });
});
