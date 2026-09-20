import type { EnvTokenVar } from '../config/contract.generated.js';

/**
 * All possible sources from which a GitHub token can originate.
 * `null` means no token was found.
 */
export type TokenSource =
  | `env:${EnvTokenVar}`
  | 'octocode-storage'
  | 'gh-cli'
  | null;
