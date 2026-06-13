export const CLIENT_NAME = 'octocode-lsp';

export const CLIENT_VERSION = resolveClientVersion();

function resolveClientVersion(): string {
  try {
    const pkg = require('../../package.json') as { version?: string };
    return pkg.version ?? '0.0.0-dev';
  } catch {
    return '0.0.0-dev';
  }
}

export const TSSERVER_LANGUAGE_IDS = new Set([
  'typescript',
  'typescriptreact',
  'javascript',
  'javascriptreact',
]);

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
