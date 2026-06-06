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
