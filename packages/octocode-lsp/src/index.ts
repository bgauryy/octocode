export { LSPClient } from './client.js';
export {
  LANGUAGE_SERVER_COMMANDS,
  detectLanguageId,
  getLanguageServerForFile,
  loadUserConfig,
  resolveLanguageServer,
  resolveTypeScriptLspProvider,
  TYPESCRIPT_LSP_PROVIDERS,
} from './config.js';
export {
  acquirePooledClient,
  getLspStatus,
  isLanguageServerAvailable,
  LSP_UNAVAILABLE_HINT,
  pooledClientCount,
  releaseAllPooledClients,
  releasePooledClientForFile,
  type LspStatusInput,
  type LspStatusResult,
} from './manager.js';
export { SymbolResolver } from './resolver.js';
export { safeReadFile, validateLSPServerPath } from './validation.js';
export { resolveWorkspaceRootForFile } from './workspaceRoot.js';
export type {
  CallHierarchyItem,
  CodeSnippet,
  ExactPosition,
  FuzzyPosition,
  IncomingCall,
  LanguageServerCommand,
  LanguageServerConfig,
  LSPPaginationInfo,
  LSPRange,
  OutgoingCall,
  ReferenceLocation,
  ReferencesByFile,
  SymbolKind,
  UserLanguageServerConfig,
} from './types.js';
