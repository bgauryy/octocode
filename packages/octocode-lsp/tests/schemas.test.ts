import { describe, expect, it } from 'vitest';

import { LSPConfigFileSchema } from '../src/schemas.js';

describe('LSPConfigFileSchema', () => {
  it('accepts valid language server config files', () => {
    expect(
      LSPConfigFileSchema.parse({
        languageServers: {
          '.foo': {
            command: 'foo-lsp',
            args: ['--stdio'],
            languageId: 'foo',
            initializationOptions: { strict: true },
          },
        },
      })
    ).toMatchObject({ languageServers: { '.foo': { command: 'foo-lsp' } } });
  });

  it('rejects invalid extension keys and control characters', () => {
    expect(() =>
      LSPConfigFileSchema.parse({
        languageServers: {
          foo: { command: 'foo-lsp', languageId: 'foo' },
        },
      })
    ).toThrow();

    expect(() =>
      LSPConfigFileSchema.parse({
        languageServers: {
          '.foo': { command: 'foo\n-lsp', languageId: 'foo' },
        },
      })
    ).toThrow();

    expect(() =>
      LSPConfigFileSchema.parse({
        languageServers: {
          '.foo': { command: 'foo-lsp', args: ['bad\rarg'], languageId: 'foo' },
        },
      })
    ).toThrow();
  });
});
