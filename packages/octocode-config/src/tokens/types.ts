/**
 * All possible sources from which a GitHub token can originate.
 * `null` means no token was found.
 */
export type TokenSource =
  | 'env:OCTOCODE_TOKEN'
  | 'env:GH_TOKEN'
  | 'env:GITHUB_TOKEN'
  | 'env:GITHUB_PERSONAL_ACCESS_TOKEN'
  | 'octocode-storage'
  | 'gh-cli'
  | null;
