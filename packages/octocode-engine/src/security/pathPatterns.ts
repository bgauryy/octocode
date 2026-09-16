import { SENSITIVE_DIRECTORY_PATTERNS } from './sensitiveDirectories.js';

// Path-access block list. Shared directory-segment policy is derived from one
// source; the remaining expressions cover platform-specific nested paths.
export const IGNORED_PATH_PATTERNS: RegExp[] = [
  ...SENSITIVE_DIRECTORY_PATTERNS,

  /(?:^|\/)\.config\/gcloud(?:\/|$)/,

  /\.mozilla\/firefox\//,
  /\.config\/chromium\//,
  /\.config\/google-chrome\//,
  /Library\/Application Support\/Google\/Chrome\//,
  /Library\/Application Support\/Firefox\//,

  /Library\/Keychains\//,
];
