/**
 * Pure builders for the LSP `initialize` request payload.
 *
 * These helpers are kept side-effect free so they can be unit-tested
 * without spawning a real language server or mocking JSON-RPC. The
 * client (`LSPClient.initialize()`) is the only call-site.
 *
 * Implements LSP best-practices identified in the May-2026 audit:
 *  - clientInfo (segment server logs / telemetry per client)
 *  - window.workDoneProgress (so servers can stream project-ready signals)
 *  - general.positionEncodings (lets servers pick utf-8 when possible)
 *  - tsserver initializationOptions tuned for headless agent usage
 *
 * @module lsp/initParams
 */
import * as path from 'path';
import type {
  InitializeParams,
  ClientCapabilities,
} from 'vscode-languageserver-protocol';
import type { LanguageServerConfig } from './types.js';
import { toUri } from './uri.js';
import {
  CLIENT_NAME,
  CLIENT_VERSION,
  TSSERVER_LANGUAGE_IDS,
  TSSERVER_DEFAULT_OPTIONS,
} from './initConstants.js';

/**
 * Build the InitializeParams for a given language-server config.
 *
 * Result shape follows the LSP 3.17 spec.
 */
export function buildInitializeParams(
  config: LanguageServerConfig
): InitializeParams {
  const rootUri = toUri(config.workspaceRoot);
  const capabilities: ClientCapabilities = {
    textDocument: {
      synchronization: {
        dynamicRegistration: true,
        willSave: false,
        willSaveWaitUntil: false,
        didSave: true,
      },
      definition: {
        dynamicRegistration: true,
        linkSupport: true,
      },
      references: {
        dynamicRegistration: true,
      },
      callHierarchy: {
        dynamicRegistration: true,
      },
      publishDiagnostics: {
        relatedInformation: true,
      },
    },
    workspace: {
      workspaceFolders: true,
      configuration: true,
    },
    window: {
      workDoneProgress: true,
    },
    general: {
      positionEncodings: ['utf-16'],
    },
  };

  return {
    processId: process.pid,
    clientInfo: {
      name: CLIENT_NAME,
      version: CLIENT_VERSION,
    },
    rootUri,
    capabilities,
    initializationOptions: buildInitializationOptions(config),
    workspaceFolders: [
      {
        uri: rootUri,
        name: path.basename(config.workspaceRoot),
      },
    ],
  };
}

/**
 * Build per-server `initializationOptions`. Returns `undefined` when the
 * target server has no tuning recommendations.
 *
 * Currently tuned for typescript-language-server only; mirrors the
 * options the VS Code extension passes (memory cap, syntax server mode,
 * disabled ATA for offline / read-only sandboxes).
 */
export function buildInitializationOptions(
  config: Pick<LanguageServerConfig, 'languageId'>
): Record<string, unknown> | undefined {
  const languageId = config.languageId;
  if (languageId && TSSERVER_LANGUAGE_IDS.has(languageId)) {
    return { ...TSSERVER_DEFAULT_OPTIONS };
  }
  return undefined;
}
