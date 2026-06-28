import { createRequire } from 'node:module';
import path from 'node:path';

import { nativeBinding } from './native.js';
import { discoverServer } from './serverDiscovery.js';
import { manifestServer, resolveCachedServer } from './serverManifest.js';
import type { LanguageServerConfig, LspServerSource } from './types.js';

const requireFromPackage = createRequire(import.meta.url);

/**
 * Pure-JS language servers shipped as npm dependencies of this package. When
 * the named command is not otherwise available we launch the bundled CLI with
 * the current Node runtime — so TS/JS, YAML, and the JSON/HTML/CSS data-format
 * servers work in production with zero user install. Keyed by the bare command
 * name the native spec table (`config.rs`) emits, so the native `args` carry
 * over unchanged.
 */
const BUNDLED_JS_SERVERS: Record<string, string> = {
  'typescript-language-server': 'typescript-language-server/lib/cli.mjs',
  'yaml-language-server': 'yaml-language-server/bin/yaml-language-server',
  'vscode-json-language-server':
    'vscode-langservers-extracted/bin/vscode-json-language-server',
  'vscode-html-language-server':
    'vscode-langservers-extracted/bin/vscode-html-language-server',
  'vscode-css-language-server':
    'vscode-langservers-extracted/bin/vscode-css-language-server',
  // PHP: native spec defaults to `intelephense` — command-keyed so native args carry over.
  'intelephense': 'intelephense/lib/intelephense.js',
};

/**
 * Bundled servers selected by languageId when the native default command
 * differs from the bundled one. Python's spec defaults to `pylsp`, but we ship
 * `pyright` — a different launch contract (`pyright-langserver --stdio`) — so
 * it is matched by language, not command name, and supplies its own args.
 */
const BUNDLED_BY_LANGUAGE: Record<string, { cli: string; args: string[] }> = {
  python: { cli: 'pyright/langserver.index.js', args: ['--stdio'] },
  // Shell scripts: native spec table has no shellscript server spec, so this
  // is the pre-native fallback path (detectLanguageId → 'shellscript' → here).
  shellscript: { cli: 'bash-language-server/out/cli.js', args: ['start'] },
};

/**
 * Names of the servers octocode bundles (offline-ready, zero install). Single
 * source for any "what's bundled" listing (e.g. the `lsp-server` CLI), derived
 * from the maps above so it never drifts. pyright is the bundled Python server
 * even though the native default command is `pylsp`.
 */
export const BUNDLED_SERVER_NAMES: readonly string[] = [
  ...Object.keys(BUNDLED_JS_SERVERS),
  'pyright',
  'bash-language-server',
];

export function detectLanguageId(filePath: string): string {
  return nativeBinding.detectLanguageId(filePath) ?? 'plaintext';
}

/**
 * Resolve the language-server config for a file, walking the resolution ladder
 * behind a stable signature. The public contract is unchanged — callers still
 * get a `LanguageServerConfig | null`; only the resolution behind it is richer.
 */
export async function getLanguageServerForFile(
  filePath: string,
  workspaceRoot: string = process.cwd()
): Promise<LanguageServerConfig | null> {
  return (await resolveServerForFile(filePath, workspaceRoot))?.config ?? null;
}

export interface ServerResolution {
  config: LanguageServerConfig;
  source: LspServerSource;
}

/**
 * Full resolution including provenance, for status reporting. Order:
 *   L0/L1 explicit override + PATH  (already applied natively in config.rs)
 *   L2    bundled JS server         (npm dep, launched via current Node)
 *   L3    project-local / ecosystem (cargo/go/python/npm-global/mason/brew…)
 *   L4    managed download cache    (~/.octocode/lsp, if pre-provisioned)
 * Returns `source: 'unavailable'` (config still populated) when nothing on the
 * machine provides the server, so the caller can report honest guidance.
 */
/** Check whether a bare command name is available on the current process PATH. */
export function isCommandOnPath(command: string): boolean {
  return nativeBinding.isCommandAvailable(command);
}

export async function resolveServerForFile(
  filePath: string,
  workspaceRoot: string = process.cwd()
): Promise<ServerResolution | null> {
  const base =
    (nativeBinding.getLanguageServerForFile(filePath, workspaceRoot) as
      | LanguageServerConfig
      | undefined) ?? null;

  // Pre-native: languages absent from the native spec table (e.g. shellscript)
  // can still get a bundled server via BUNDLED_BY_LANGUAGE.
  if (!base) {
    const langId = nativeBinding.detectLanguageId(filePath) ?? '';
    const preNative = langId ? BUNDLED_BY_LANGUAGE[langId] : undefined;
    if (preNative) {
      const cliPath = resolveBundledCli(preNative.cli);
      if (cliPath) {
        return {
          config: {
            command: process.execPath,
            args: [cliPath, ...preNative.args],
            languageId: langId,
          } as LanguageServerConfig,
          source: 'bundled',
        };
      }
    }
    return null;
  }

  // L0/L1: the native command already resolved (explicit override or on PATH).
  if (nativeBinding.isCommandAvailable(base.command)) {
    return { config: base, source: 'path' };
  }

  // L2: bundled JS server launched with the current Node runtime.
  const bundled = withBundledJsServer(base);
  if (bundled) return { config: bundled, source: 'bundled' };

  // L3: discover a server installed outside PATH (ecosystem/project-local dirs).
  const discovered = discoverServer(base.command, workspaceRoot);
  if (discovered) {
    return {
      config: { ...base, command: discovered.command },
      source: discovered.source === 'project-local' ? 'project-local' : 'ecosystem',
    };
  }

  // L4: a binary already sitting in the managed download cache.
  const cached = resolveCachedServer(base.command);
  if (cached) {
    const manifest = manifestServer(base.command);
    return {
      config: {
        ...base,
        command: cached,
        args: manifest?.launchArgs ?? base.args,
      },
      source: 'managed-cache',
    };
  }

  return { config: base, source: 'unavailable' };
}

function withBundledJsServer(
  config: LanguageServerConfig
): LanguageServerConfig | null {
  // 1) Command-keyed: native command name matches the bundled package; keep
  //    the native args.
  const byCommand = BUNDLED_JS_SERVERS[path.basename(config.command)];
  if (byCommand) {
    const cliPath = resolveBundledCli(byCommand);
    if (cliPath) {
      return {
        ...config,
        command: process.execPath,
        args: [cliPath, ...(config.args ?? [])],
      };
    }
  }

  // 2) Language-keyed: a different bundled server than the native default
  //    (e.g. pyright for Python); use the bundled server's own launch args.
  const byLanguage = config.languageId
    ? BUNDLED_BY_LANGUAGE[config.languageId]
    : undefined;
  if (byLanguage) {
    const cliPath = resolveBundledCli(byLanguage.cli);
    if (cliPath) {
      return {
        ...config,
        command: process.execPath,
        args: [cliPath, ...byLanguage.args],
      };
    }
  }

  return null;
}

function resolveBundledCli(modulePath: string): string | null {
  try {
    return requireFromPackage.resolve(modulePath);
  } catch {
    return null;
  }
}
