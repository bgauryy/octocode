// Hand-authored ESM facade for the engine. The package is "type": "module",
// so the `import` / `default` export conditions resolve to this file. It must
// stay ESM (import/export) — never the napi-rs CJS loader. The native binary is
// loaded once by ./index.cjs (the `require` condition) and re-exported here.
//
// This is a CANONICAL SOURCE under loader/. `napi build` regenerates the root
// index.js as a CJS loader; postbuild.cjs overwrites it back from this file.
// When new native symbols are added in Rust, add matching `export const` lines.
import nativeBinding from './index.cjs';

export const SIGNATURES_ONLY_HINT = nativeBinding.SIGNATURES_ONLY_HINT;
export const minifyContent = nativeBinding.minifyContent;
export const applyContentViewMinification =
  nativeBinding.applyContentViewMinification;
export const extractSignatures = nativeBinding.extractSignatures;
export const extractJsSymbols = nativeBinding.extractJsSymbols;
export const findInFileReferences = nativeBinding.findInFileReferences;
export const extractGraphFacts = nativeBinding.extractGraphFacts;
export const scanGraphFacts = nativeBinding.scanGraphFacts;
export const getSupportedJsTsExtensions =
  nativeBinding.getSupportedJsTsExtensions;
export const getSupportedGraphFactExtensions =
  nativeBinding.getSupportedGraphFactExtensions;
export const getGraphFactCapabilities = nativeBinding.getGraphFactCapabilities;
export const getGrammarCapabilities = nativeBinding.getGrammarCapabilities;
export const structuralSearch = nativeBinding.structuralSearch;
export const structuralSearchDetailed = nativeBinding.structuralSearchDetailed;
export const structuralSearchFiles = nativeBinding.structuralSearchFiles;
export const structuralSearchFilesDetailed =
  nativeBinding.structuralSearchFilesDetailed;
export const getSupportedStructuralExtensions =
  nativeBinding.getSupportedStructuralExtensions;
export const getSemanticBoundaryOffsets =
  nativeBinding.getSemanticBoundaryOffsets;
export const inspectSyntaxTree = nativeBinding.inspectSyntaxTree;

export const getSupportedSignatureExtensions =
  nativeBinding.getSupportedSignatureExtensions;
export const jsonToYamlString = nativeBinding.jsonToYamlString;
export const SUPPORTED_SIGNATURE_EXTENSIONS =
  nativeBinding.SUPPORTED_SIGNATURE_EXTENSIONS;
export const SUPPORTED_GRAPH_FACT_EXTENSIONS =
  nativeBinding.SUPPORTED_GRAPH_FACT_EXTENSIONS;
export const SUPPORTED_STRUCTURAL_EXTENSIONS =
  nativeBinding.SUPPORTED_STRUCTURAL_EXTENSIONS;
export const parseRipgrepJson = nativeBinding.parseRipgrepJson;
export const searchRipgrep = nativeBinding.searchRipgrep;
export const validateRipgrepPattern = nativeBinding.validateRipgrepPattern;
export const queryFileSystem = nativeBinding.queryFileSystem;
export const buildIndex = nativeBinding.buildIndex;
export const queryIndex = nativeBinding.queryIndex;
export const indexStatus = nativeBinding.indexStatus;
export const charToByteOffset = nativeBinding.charToByteOffset;
export const byteToCharOffset = nativeBinding.byteToCharOffset;
export const byteSliceContent = nativeBinding.byteSliceContent;
export const sliceContent = nativeBinding.sliceContent;
export const extractMatchingLines = nativeBinding.extractMatchingLines;
export const filterPatch = nativeBinding.filterPatch;
export const PatchLineType = nativeBinding.PatchLineType;
export const NativeLspClient = nativeBinding.NativeLspClient;
export const configureLspClientPool = nativeBinding.configureLspClientPool;
export const acquirePooledLspClient = nativeBinding.acquirePooledLspClient;
export const releasePooledLspClient = nativeBinding.releasePooledLspClient;
export const clearPooledLspClients = nativeBinding.clearPooledLspClients;
export const pooledLspClientCount = nativeBinding.pooledLspClientCount;
export const pooledLspClientConfigs = nativeBinding.pooledLspClientConfigs;
export const resolvePosition = nativeBinding.resolvePosition;
export const resolvePositionFromContent =
  nativeBinding.resolvePositionFromContent;
export const toUri = nativeBinding.toUri;
export const fromUri = nativeBinding.fromUri;
export const resolveWorkspaceRootForFile =
  nativeBinding.resolveWorkspaceRootForFile;
export const detectLanguageId = nativeBinding.detectLanguageId;
export const getLanguageServerForFile = nativeBinding.getLanguageServerForFile;
export const isCommandAvailable = nativeBinding.isCommandAvailable;
export const safeReadFile = nativeBinding.safeReadFile;
export const safeReadLineWindow = nativeBinding.safeReadLineWindow;
export const validateLspServerPath = nativeBinding.validateLspServerPath;
export const convertSymbolKind = nativeBinding.convertSymbolKind;
export const toLspSymbolKind = nativeBinding.toLspSymbolKind;
export const sanitizeContent = nativeBinding.sanitizeContent;
export const maskSensitiveData = nativeBinding.maskSensitiveData;

export default nativeBinding;
