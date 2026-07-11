/**
 * Barrel for the semanticFileOps/ split (anchor & workspace resolution vs.
 * documentSymbols vs. workspaceSymbols/diagnostics operations). Kept so no
 * other file in the repo needs to change its import path.
 */
export {
  isNativeJsTsFile,
  throwLspUnavailable,
  toLocalPath,
  workspaceSymbolAnchorExtensions,
  workspaceSymbolAnchorIncludeGlobs,
  findWorkspaceSymbolAnchorByName,
  resolveWorkspaceSymbolAnchor,
  lspErrorMessage,
  nativeDocumentSymbols,
} from './semanticFileOps/anchor.js';

export {
  type CompactSymbol,
  getDocumentSymbols,
  flattenDocumentSymbols,
  flattenDocumentSymbol,
  getSymbolRange,
  isLspRange,
  isPosition,
  countTopLevelDocumentSymbols,
  countBy,
} from './semanticFileOps/documentSymbols.js';

export {
  getWorkspaceSymbols,
  compactWorkspaceSymbols,
  getFileDiagnostics,
  extractDiagnostics,
  parseDiagnostic,
} from './semanticFileOps/workspaceSymbolsAndDiagnostics.js';
