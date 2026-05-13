import type { SensitiveDataPattern } from './types.js';

export const versionControlPatterns: SensitiveDataPattern[] = [
  {
    name: 'gitlabPersonalAccessToken',
    description: 'GitLab personal access token',
    regex: /\bglpat-[A-Za-z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'gitlabDeployToken',
    description: 'GitLab deploy token',
    regex: /\bgldt-[A-Za-z0-9_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'gitlabRunnerToken',
    description: 'GitLab runner registration token',
    regex: /\bglrt-[A-Za-z0-9_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'gitlabCiJobToken',
    description: 'GitLab CI/CD job token',
    regex: /\bglcbt-[0-9a-zA-Z]{1,5}_[0-9a-zA-Z_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  // NOTE: gitlabRunnerAuthToken removed - identical regex to gitlabRunnerToken above (glrt-[A-Za-z0-9_-]{20})
  {
    name: 'gitlabPipelineTriggerToken',
    description: 'GitLab pipeline trigger token',
    regex: /\bglptt-[0-9a-f]{40}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'bitbucketAppPassword',
    description: 'Bitbucket app password',
    regex: /\bATBB[a-zA-Z0-9]{24}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'githubTokens',
    description: 'GitHub personal access token (classic)',
    regex: /\b((?:ghp|gho|ghu|ghs|ghr|github_pat)_[a-zA-Z0-9_]{36,255})\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'githubFineGrainedToken',
    description: 'GitHub fine-grained personal access token',
    regex: /\bgithub_pat_[A-Za-z0-9_]{82}\b/g,
    matchAccuracy: 'high',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
  {
    name: 'githubAppInstallationToken',
    description: 'GitHub App installation token',
    regex: /\bghs_[0-9a-zA-Z]{37}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab SCIM Token
  {
    name: 'gitlabScimToken',
    description: 'GitLab SCIM token',
    regex: /\bglsoat-[0-9a-zA-Z_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab Feature Flag Client Token
  {
    name: 'gitlabFeatureFlagToken',
    description: 'GitLab feature flag client token',
    regex: /\bglffct-[0-9a-zA-Z_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab Feed Token
  {
    name: 'gitlabFeedToken',
    description: 'GitLab feed token',
    regex: /\bglft-[0-9a-zA-Z_-]{20}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab Incoming Mail Token
  {
    name: 'gitlabIncomingMailToken',
    description: 'GitLab incoming mail token',
    regex: /\bglimt-[0-9a-zA-Z_-]{25}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab Kubernetes Agent Token
  {
    name: 'gitlabK8sAgentToken',
    description: 'GitLab Kubernetes agent token',
    regex: /\bglagent-[0-9a-zA-Z_-]{50}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab OAuth App Secret
  {
    name: 'gitlabOAuthAppSecret',
    description: 'GitLab OAuth application secret',
    regex: /\bgloas-[0-9a-zA-Z_-]{64}\b/g,
    matchAccuracy: 'high',
  },
  // GitLab Session Cookie
  {
    name: 'gitlabSessionCookie',
    description: 'GitLab session cookie',
    regex: /_gitlab_session=[0-9a-z]{32}/g,
    matchAccuracy: 'high',
  },
  // Bitbucket Repository Token
  {
    name: 'bitbucketRepoToken',
    description: 'Bitbucket repository access token',
    regex: /\bATCTT3[a-zA-Z0-9]{24}\b/g,
    matchAccuracy: 'high',
  },
];
