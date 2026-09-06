import { describe, expect, it } from 'vitest';
import {
  applyRustBuildContext,
  serverConfigurationFingerprint,
} from '../../src/lsp/rustContext.js';
import { serializeKey } from '../../src/lsp/lspClientPool.js';

const config = {
  command: 'rust-analyzer',
  workspaceRoot: '/workspace',
  languageId: 'rust',
};

describe('Rust semantic build contexts', () => {
  it('canonicalizes features and cfgs and disables executable providers by default', () => {
    const first = applyRustBuildContext(config, {
      features: ['b', 'a', 'a'],
      cfgs: ['custom', 'mode=fast'],
    });
    const second = applyRustBuildContext(config, {
      cfgs: ['mode=fast', 'custom'],
      features: ['a', 'b'],
    });
    expect(first.initializationOptions).toMatchObject({
      cargo: {
        features: ['a', 'b'],
        cfgs: ['custom', 'mode=fast'],
        buildScripts: { enable: false },
      },
      procMacro: { enable: false },
      checkOnSave: false,
      cfg: { setTest: false },
    });
    expect(serverConfigurationFingerprint(first)).toBe(
      serverConfigurationFingerprint(second)
    );
  });

  it('partitions server identity by every effective build setting', () => {
    const baseline = serverConfigurationFingerprint(
      applyRustBuildContext(config, {})
    );
    for (const context of [
      { features: ['a'] },
      { features: 'all' as const },
      { cfgs: ['custom'] },
      { target: 'aarch64-apple-darwin' },
      { noDefaultFeatures: true },
      { buildScripts: true },
      { buildScripts: true, procMacros: true },
    ]) {
      const contextFingerprint = serverConfigurationFingerprint(
        applyRustBuildContext(config, context)
      );
      expect(contextFingerprint).not.toBe(baseline);
      const key = {
        ...config,
        filePath: '/workspace/main.rs',
        languageId: 'rust',
        contextFingerprint,
      };
      expect(serializeKey(key)).not.toBe(
        serializeKey({ ...key, contextFingerprint: baseline })
      );
    }
  });

  it('rejects an impossible execution policy and non-Rust contexts', () => {
    expect(() => applyRustBuildContext(config, { procMacros: true })).toThrow(
      'buildScripts'
    );
    expect(() =>
      applyRustBuildContext({ ...config, languageId: 'typescript' }, {})
    ).toThrow('Rust');
  });

  it('includes custom server initialization and environment in pool identity', () => {
    expect(
      serverConfigurationFingerprint({
        ...config,
        env: { RUSTFLAGS: '--cfg a' },
      })
    ).not.toBe(
      serverConfigurationFingerprint({
        ...config,
        env: { RUSTFLAGS: '--cfg b' },
      })
    );
    expect(
      serverConfigurationFingerprint({
        ...config,
        initializationOptions: { cargo: { features: ['a'] } },
      })
    ).not.toBe(serverConfigurationFingerprint(config));
  });
});
