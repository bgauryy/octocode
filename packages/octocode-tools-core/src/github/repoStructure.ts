// Barrel: implementation split across src/github/repoStructure/* to satisfy
// the max-lines:400 lint rule. Re-exports the public entry point so existing
// consumers importing from './repoStructure.js' are unaffected.
export { viewGitHubRepositoryStructureAPI } from './repoStructure/fetchOrchestration.js';
