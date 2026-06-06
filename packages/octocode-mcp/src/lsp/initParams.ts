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

export function buildInitializationOptions(
  config: Pick<LanguageServerConfig, 'languageId'>
): Record<string, unknown> | undefined {
  const languageId = config.languageId;
  if (languageId && TSSERVER_LANGUAGE_IDS.has(languageId)) {
    return { ...TSSERVER_DEFAULT_OPTIONS };
  }
  return undefined;
}
