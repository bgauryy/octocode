// Thin barrel: the real implementation lives in ./npm/*.ts (split to satisfy
// max-lines lint). This file exists only to keep external import paths
// (`utils/package/npm.js`) stable — do not add logic here.
export {
  getNpmRegistryUrl,
  _resetNpmRegistryUrlCache,
  checkNpmRegistryReachable,
} from './npm/npmRegistry.js';
export {
  isExactPackageName,
  _packageNameToSearchKeywords,
  searchNpmPackage,
  checkNpmDeprecation,
} from './npm/npmDeprecation.js';
