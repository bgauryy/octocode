/**
 * Octocode credential constants.
 *
 * OCTOCODE_GITHUB_APP_CLIENT_ID: The GitHub OAuth App client ID registered for
 * Octocode at https://github.com/settings/applications.
 * Used for all OAuth flows (initial auth, token refresh) and as the default
 * clientId throughout the credentials module. Not a secret — client IDs are
 * public identifiers embedded in OAuth flows.
 */

export const OCTOCODE_GITHUB_APP_CLIENT_ID = '178c6fc778ccc68e1d6a';
export const DEFAULT_HOSTNAME = 'github.com';
