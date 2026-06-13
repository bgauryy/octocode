import { afterEach, describe, expect, it } from 'vitest';
import {
  resolveLanguageServer,
  resolveTypeScriptLspProvider,
} from '../src/config.js';

const TS_SERVER_CONFIG = {
  command: 'typescript-language-server',
  args: ['--stdio'],
  envVar: 'OCTOCODE_TS_SERVER_PATH',
};

const ORIGINAL_PROVIDER = process.env.OCTOCODE_TS_LSP_PROVIDER;
const ORIGINAL_SERVER_PATH = process.env.OCTOCODE_TS_SERVER_PATH;

afterEach(() => {
  restoreEnv('OCTOCODE_TS_LSP_PROVIDER', ORIGINAL_PROVIDER);
  restoreEnv('OCTOCODE_TS_SERVER_PATH', ORIGINAL_SERVER_PATH);
});

describe('TypeScript LSP provider selection', () => {
  it('uses typescript-language-server as the stable default', () => {
    delete process.env.OCTOCODE_TS_LSP_PROVIDER;
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveTypeScriptLspProvider()).toBe('typescript-language-server');
  });

  it('falls back to the stable default for unknown provider values', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'fast-but-imaginary';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveTypeScriptLspProvider()).toBe('typescript-language-server');
  });

  it('resolves tsgo to its stdio LSP subcommand', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'tsgo';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: 'tsgo',
      args: ['lsp', '--stdio'],
    });
  });

  it('resolves vtsls to stdio mode', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'vtsls';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: 'vtsls',
      args: ['--stdio'],
    });
  });

  it('preserves OCTOCODE_TS_SERVER_PATH as the highest-priority override', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'tsgo';
    process.env.OCTOCODE_TS_SERVER_PATH = '/custom/typescript-language-server';

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: '/custom/typescript-language-server',
      args: ['--stdio'],
    });
  });
});

function restoreEnv(key: string, value: string | undefined): void {
  if (value === undefined) {
    delete process.env[key];
  } else {
    process.env[key] = value;
  }
}
