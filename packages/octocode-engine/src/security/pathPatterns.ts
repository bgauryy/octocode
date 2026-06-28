// Path-access block list: prevents reading from sensitive directories.
// SYNC NOTE: discoveryFilter.ts:DISCOVERY_IGNORED_FOLDER_NAMES overlaps this
// list (e.g. .git, .aws, .ssh, .docker, .kube). Both lists must be kept in
// sync — changes here that protect against directory traversal attacks should
// be reflected there, and vice versa.
export const IGNORED_PATH_PATTERNS: RegExp[] = [
  /(?:^|\/)\.git(?:\/|$)/,

  /(?:^|\/)\.ssh(?:\/|$)/,

  /(?:^|\/)\.aws(?:\/|$)/,

  /(?:^|\/)\.docker(?:\/|$)/,

  /(?:^|\/)\.config\/gcloud(?:\/|$)/,

  /(?:^|\/)\.azure(?:\/|$)/,

  /(?:^|\/)\.kube(?:\/|$)/,

  /(?:^|\/)\.terraform(?:\/|$)/,

  /(?:^|\/)secrets(?:\/|$)/,
  /(?:^|\/)private(?:\/|$)/,

  /(?:^|\/)\.password-store(?:\/|$)/,

  /\.mozilla\/firefox\//,
  /\.config\/chromium\//,
  /\.config\/google-chrome\//,
  /Library\/Application Support\/Google\/Chrome\//,
  /Library\/Application Support\/Firefox\//,

  /Library\/Keychains\//,

  /(?:^|\/)\.thunderbird(?:\/|$)/,
  /(?:^|\/)\.evolution(?:\/|$)/,

  /(?:^|\/)\.vagrant(?:\/|$)/,
  /(?:^|\/)\.minikube(?:\/|$)/,

  /(?:^|\/)\.bitcoin(?:\/|$)/,
  /(?:^|\/)\.ethereum(?:\/|$)/,
  /(?:^|\/)\.electrum(?:\/|$)/,
];
