export { LSPClient } from './client.js';
export {
  detectLanguageId,
  getLanguageServerForFile,
  isCommandOnPath,
  resolveServerForFile,
  BUNDLED_SERVER_NAMES,
  type ServerResolution,
} from './config.js';
export { detectPlatformId, isMuslLinux, type PlatformId } from './platform.js';
export {
  detectIdeContext,
  type IdeContext,
  type IdeHost,
} from './ideContext.js';
export {
  clearDiscoveryCache,
  discoverServer,
  discoverServerBatch,
  type DiscoveredServer,
  type DiscoverySource,
} from './serverDiscovery.js';
export {
  cachedServerBinPath,
  isAutoDownloadable,
  listManifestServers,
  managedCacheRoot,
  manifestInstallHint,
  manifestServer,
  provisionMode,
  resolveCachedServer,
  type ArchiveKind,
  type ManifestAsset,
  type ManifestServer,
  type ProvisionMode,
} from './serverManifest.js';
export {
  provisionServer,
  uninstallServer,
  type ProvisionResult,
} from './serverProvisioner.js';
export {
  acquirePooledClient,
  getLspStatus,
  isLanguageServerAvailable,
  LSP_UNAVAILABLE_HINT,
  pooledClientCount,
  releaseAllPooledClients,
  releasePooledClientForFile,
  unavailableHintFor,
  TOOLCHAIN_SERVERS,
  type ToolchainServer,
  type LspStatusInput,
  type LspStatusResult,
} from './manager.js';
export {
  resolveImportAliasDefinitions,
  SymbolResolver,
  type ImportAliasDefinitionInput,
} from './resolver.js';
export { safeReadFile, validateLSPServerPath } from './validation.js';
export { resolveWorkspaceRootForFile } from './workspaceRoot.js';
export type {
  CallHierarchyItem,
  CodeSnippet,
  ExactPosition,
  FuzzyPosition,
  IncomingCall,
  InitializationOptions,
  LanguageServerCommand,
  LanguageServerConfig,
  LSPPaginationInfo,
  LSPRange,
  LspServerSource,
  OutgoingCall,
  ReferenceLocation,
  ReferencesByFile,
  SymbolKind,
  UserLanguageServerConfig,
} from './types.js';
