// Barrel: re-exports the install-time interactive prompts.
// Implementation is split across src/ui/install/prompts/* to keep files
// under the max-lines lint limit. No behavior lives in this file.
export { selectMCPClient } from './prompts/client-select.js';
export { promptLocalTools } from './prompts/local-tools.js';
export { promptGitHubAuth } from './prompts/github-auth.js';
