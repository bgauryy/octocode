import { describe, expect, it } from 'vitest';

import {
  buildInitializeParams,
  buildInitializationOptions,
} from '../../src/lsp/initParams.js';
import { fromUriSafe, UnsafeUriError } from '../../src/lsp/uri.js';
import { LSP_ERROR_CODES } from '../../src/lsp/lspErrorCodes.js';

describe('T1.1 — Initialize handshake includes industry-standard fields', () => {
  const params = buildInitializeParams({
    command: 'typescript-language-server',
    workspaceRoot: '/workspaces/repo',
    languageId: 'typescript',
  });

  it('declares clientInfo so the language server can log/segment by client', () => {
    expect(params.clientInfo).toBeDefined();
    expect(params.clientInfo?.name).toMatch(/octocode/i);
    expect(typeof params.clientInfo?.version).toBe('string');
  });

  it('advertises window.workDoneProgress so servers can stream project-ready signals', () => {
    expect(params.capabilities.window?.workDoneProgress).toBe(true);
  });

  it('lists supported positionEncodings so the server can pick utf-8 over utf-16 when possible', () => {
    const general = params.capabilities.general as
      | { positionEncodings?: string[] }
      | undefined;
    expect(general?.positionEncodings).toContain('utf-16');
  });

  it('keeps the existing capability bundle (definition / references / callHierarchy)', () => {
    expect(params.capabilities.textDocument?.definition).toBeDefined();
    expect(params.capabilities.textDocument?.references).toBeDefined();
    expect(params.capabilities.textDocument?.callHierarchy).toBeDefined();
  });
});

describe('T1.2 — tsserver receives initializationOptions tuned for agent use', () => {
  it('passes tuned options when the language is typescript', () => {
    const params = buildInitializeParams({
      command: 'typescript-language-server',
      workspaceRoot: '/workspaces/repo',
      languageId: 'typescript',
    });
    const opts = params.initializationOptions as
      | {
          tsserver?: {
            maxTsServerMemory?: number;
            useSyntaxServer?: string;
            disableAutomaticTypeAcquisition?: boolean;
          };
          preferences?: { includePackageJsonAutoImports?: string };
        }
      | undefined;

    expect(opts?.tsserver?.maxTsServerMemory).toBeGreaterThanOrEqual(2048);
    expect(opts?.tsserver?.useSyntaxServer).toBe('auto');
    expect(opts?.tsserver?.disableAutomaticTypeAcquisition).toBe(true);
    expect(opts?.preferences?.includePackageJsonAutoImports).toBe('off');
  });

  it('passes the same tuned options for tsx / javascript variants', () => {
    for (const languageId of [
      'javascript',
      'typescriptreact',
      'javascriptreact',
    ]) {
      const params = buildInitializeParams({
        command: 'typescript-language-server',
        workspaceRoot: '/workspaces/repo',
        languageId,
      });
      const opts = params.initializationOptions as
        | { tsserver?: { maxTsServerMemory?: number } }
        | undefined;
      expect(opts?.tsserver?.maxTsServerMemory).toBeGreaterThanOrEqual(2048);
    }
  });

  it('omits tsserver options when the language is not in the typescript family', () => {
    const params = buildInitializeParams({
      command: 'gopls',
      workspaceRoot: '/workspaces/repo',
      languageId: 'go',
    });
    const opts = params.initializationOptions as
      | { tsserver?: unknown }
      | undefined;

    expect(opts?.tsserver).toBeUndefined();
  });

  it('buildInitializationOptions is exposed for unit testing', () => {
    expect(
      buildInitializationOptions({ languageId: 'typescript' })
    ).toMatchObject({ tsserver: { useSyntaxServer: 'auto' } });
    expect(buildInitializationOptions({ languageId: 'rust' })).toBeUndefined();
  });
});

describe('T1.5 — fromUri hardening (defence against malicious LSP responses)', () => {
  it('parses well-formed file URIs', () => {
    const result = fromUriSafe('file:///workspaces/repo/src/index.ts');
    expect(result.isValid).toBe(true);
    expect(result.path).toMatch(/index\.ts$/);
  });

  it('rejects non-file schemes (http, https, untitled, etc.)', () => {
    for (const uri of [
      'http://example.com/foo.ts',
      'https://example.com/foo.ts',
      'untitled:Untitled-1',
      'inmemory://model/1',
      'javascript:alert(1)',
    ]) {
      const result = fromUriSafe(uri);
      expect(result.isValid).toBe(false);
      expect(result.error).toMatch(/scheme/i);
    }
  });

  it('rejects bare path strings (must be a real URI)', () => {
    const result = fromUriSafe('/etc/passwd');
    expect(result.isValid).toBe(false);
  });

  it('rejects URIs with embedded null bytes', () => {
    const result = fromUriSafe('file:///workspaces/repo/\u0000evil.ts');
    expect(result.isValid).toBe(false);
  });

  it('throws UnsafeUriError when a strict variant is needed', () => {
    expect(() =>
      (fromUriSafe as any)('http://evil.com/x', { throwOnInvalid: true })
    ).toThrow(UnsafeUriError);
  });
});

describe('T2.1 — Structured LSP error code enum', () => {
  it('exports a stable, agent-facing error-code taxonomy', () => {
    expect(LSP_ERROR_CODES).toMatchObject({
      LSP_NOT_INSTALLED: expect.any(String),
      LSP_TIMEOUT: expect.any(String),
      LSP_INITIALIZE_FAILED: expect.any(String),
      LSP_REQUEST_FAILED: expect.any(String),
      LSP_EMPTY: expect.any(String),
      LSP_FALLBACK_TO_TEXT: expect.any(String),
      SYMBOL_NOT_FOUND: expect.any(String),
      SYMBOL_AMBIGUOUS: expect.any(String),
      UNSAFE_URI: expect.any(String),
    });
  });

  it('every error code is a SCREAMING_SNAKE_CASE string (stable wire format)', () => {
    for (const code of Object.values(LSP_ERROR_CODES)) {
      expect(code).toMatch(/^[A-Z][A-Z0-9_]*$/);
    }
  });
});
