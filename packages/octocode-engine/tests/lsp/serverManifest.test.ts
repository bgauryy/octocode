import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { homedir, tmpdir } from 'node:os';
import { afterEach, describe, expect, it } from 'vitest';

import {
  cachedServerBinPath,
  isAutoDownloadable,
  managedCacheRoot,
  manifestInstallHint,
  manifestServer,
  provisionMode,
  resolveCachedServer,
} from '../../src/lsp/serverManifest.js';

const ORIGINAL = process.env.OCTOCODE_LSP_AUTO_INSTALL;
const ORIGINAL_CACHE = process.env.OCTOCODE_LSP_CACHE_DIR;

afterEach(() => {
  if (ORIGINAL === undefined) delete process.env.OCTOCODE_LSP_AUTO_INSTALL;
  else process.env.OCTOCODE_LSP_AUTO_INSTALL = ORIGINAL;
  if (ORIGINAL_CACHE === undefined) delete process.env.OCTOCODE_LSP_CACHE_DIR;
  else process.env.OCTOCODE_LSP_CACHE_DIR = ORIGINAL_CACHE;
});

describe('serverManifest', () => {
  it('exposes the auto-downloadable servers from the manifest', () => {
    expect(isAutoDownloadable('rust-analyzer')).toBe(true);
    expect(isAutoDownloadable('clangd')).toBe(true);
    // toolchain-coupled / bundled servers are intentionally absent
    expect(isAutoDownloadable('gopls')).toBe(false);
    expect(isAutoDownloadable('pyright-langserver')).toBe(false);
  });

  it('resolves a manifest entry by bare command name', () => {
    const server = manifestServer('rust-analyzer');
    expect(server?.languageId).toBe('rust');
    expect(server?.repo).toBe('rust-lang/rust-analyzer');
    expect(server?.platforms['darwin-arm64']?.url).toContain('aarch64-apple-darwin');
  });

  it('pins sha256 per asset so downloads are integrity-verified', () => {
    const server = manifestServer('rust-analyzer');
    const assets = Object.values(server?.platforms ?? {});
    expect(assets.length).toBeGreaterThan(0);
    for (const asset of assets) {
      expect(asset?.sha256).toMatch(/^[0-9a-f]{64}$/);
    }
  });

  it('defaults provision mode to prompt and parses valid modes', () => {
    delete process.env.OCTOCODE_LSP_AUTO_INSTALL;
    expect(provisionMode()).toBe('prompt');
    expect(provisionMode({ OCTOCODE_LSP_AUTO_INSTALL: 'auto' })).toBe('auto');
    expect(provisionMode({ OCTOCODE_LSP_AUTO_INSTALL: 'off' })).toBe('off');
    expect(provisionMode({ OCTOCODE_LSP_AUTO_INSTALL: 'prompt' })).toBe('prompt');
    // Unrecognized values fall back to the default (prompt), not off.
    expect(provisionMode({ OCTOCODE_LSP_AUTO_INSTALL: 'garbage' })).toBe('prompt');
  });

  it('locates the managed cache under ~/.octocode/lsp', () => {
    expect(managedCacheRoot()).toBe(path.join(homedir(), '.octocode', 'lsp'));
    const binPath = cachedServerBinPath('rust-analyzer', 'linux-x64');
    expect(binPath).toContain(path.join('.octocode', 'lsp', 'rust-analyzer'));
  });

  it('returns an enable hint pointing at OCTOCODE_LSP_AUTO_INSTALL when downloads are off', () => {
    process.env.OCTOCODE_LSP_AUTO_INSTALL = 'off';
    const hint = manifestInstallHint('rust-analyzer');
    expect(hint).toContain('OCTOCODE_LSP_AUTO_INSTALL');
    // Non-auto-downloadable (toolchain) servers have no manifest hint.
    expect(manifestInstallHint('gopls')).toBeNull();
  });

  it('honors OCTOCODE_LSP_CACHE_DIR override for the cache root', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'octocode-lsp-cache-'));
    process.env.OCTOCODE_LSP_CACHE_DIR = dir;
    expect(managedCacheRoot()).toBe(path.resolve(dir));
    rmSync(dir, { recursive: true, force: true });
  });

  it('resolves a cached server only when the .ok completion marker exists', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'octocode-lsp-cache-'));
    process.env.OCTOCODE_LSP_CACHE_DIR = dir;
    const binPath = cachedServerBinPath('rust-analyzer', 'linux-x64');
    expect(binPath).toBeTruthy();
    mkdirSync(path.dirname(binPath!), { recursive: true });
    writeFileSync(binPath!, 'binary\n');

    // Binary present but no marker → treated as a partial/corrupt install.
    expect(resolveCachedServer('rust-analyzer', 'linux-x64')).toBeNull();

    // Marker written last → now trusted.
    writeFileSync(`${binPath}.ok`, 'sha256\n');
    expect(resolveCachedServer('rust-analyzer', 'linux-x64')).toBe(binPath);

    rmSync(dir, { recursive: true, force: true });
  });
});
