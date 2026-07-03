import { describe, expect, it } from 'vitest';
import { LSP_ERROR_CODES } from '../../src/lsp/lspErrorCodes.js';

describe('LSP_ERROR_CODES', () => {
  it('is a const object with string values', () => {
    for (const value of Object.values(LSP_ERROR_CODES)) {
      expect(typeof value).toBe('string');
      expect(value.length).toBeGreaterThan(0);
    }
  });

  it('contains all expected error codes', () => {
    expect(LSP_ERROR_CODES.LSP_NOT_INSTALLED).toBe('LSP_NOT_INSTALLED');
    expect(LSP_ERROR_CODES.LSP_TIMEOUT).toBe('LSP_TIMEOUT');
    expect(LSP_ERROR_CODES.LSP_INITIALIZE_FAILED).toBe('LSP_INITIALIZE_FAILED');
    expect(LSP_ERROR_CODES.LSP_REQUEST_FAILED).toBe('LSP_REQUEST_FAILED');
    expect(LSP_ERROR_CODES.LSP_EMPTY).toBe('LSP_EMPTY');
    expect(LSP_ERROR_CODES.LSP_CAPABILITY_UNSUPPORTED).toBe('LSP_CAPABILITY_UNSUPPORTED');
    expect(LSP_ERROR_CODES.LSP_FALLBACK_TO_TEXT).toBe('LSP_FALLBACK_TO_TEXT');
    expect(LSP_ERROR_CODES.SYMBOL_NOT_FOUND).toBe('SYMBOL_NOT_FOUND');
    expect(LSP_ERROR_CODES.SYMBOL_AMBIGUOUS).toBe('SYMBOL_AMBIGUOUS');
    expect(LSP_ERROR_CODES.UNSAFE_URI).toBe('UNSAFE_URI');
  });

  it('has exactly the declared set of codes (no undocumented additions)', () => {
    const keys = Object.keys(LSP_ERROR_CODES);
    expect(keys).toHaveLength(10);
  });
});
