/**
 * Constants for the LSP initialize handshake.
 *
 * Pulled into their own module so that `initParams.ts` stays pure and
 * `lsp_provenance.test.ts`-style source-pinning tests are stable.
 *
 * @module lsp/initConstants
 */

/** Client name advertised to language servers (LSP `clientInfo.name`). */
export const CLIENT_NAME = 'octocode-mcp';

/**
 * Client version — best-effort. Defaults to '0.0.0-dev' so we never
 * crash if the package.json version is missing in odd build environments.
 */
export const CLIENT_VERSION = resolveClientVersion();

function resolveClientVersion(): string {
  try {
    // Inline require to avoid pulling fs/path at module init.
    // Falls back if the import doesn't resolve (e.g. in some bundle setups).

    const pkg = require('../../package.json') as { version?: string };
    return pkg.version ?? '0.0.0-dev';
  } catch {
    return '0.0.0-dev';
  }
}

/** Language IDs handled by typescript-language-server. */
export const TSSERVER_LANGUAGE_IDS = new Set([
  'typescript',
  'typescriptreact',
  'javascript',
  'javascriptreact',
]);

/**
 * Default tsserver options for agent / batch use.
 *
 * - 2GB max memory: keeps tsserver alive on large monorepos.
 * - `useSyntaxServer: 'auto'`: let tsserver pick light syntax mode for
 *   files we don't need full semantics on.
 * - `disableAutomaticTypeAcquisition: true`: agents work in offline /
 *   read-only sandboxes; ATA would hit the network with no consent.
 * - `includePackageJsonAutoImports: 'off'`: suppress an O(packages)
 *   indexing pass we never use (we don't surface completions).
 */
export const TSSERVER_DEFAULT_OPTIONS: Record<string, unknown> = {
  tsserver: {
    maxTsServerMemory: 2048,
    useSyntaxServer: 'auto',
    disableAutomaticTypeAcquisition: true,
  },
  preferences: {
    includePackageJsonAutoImports: 'off',
  },
};
