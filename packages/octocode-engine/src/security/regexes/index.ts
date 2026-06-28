// JS fallback pattern registry — used when the native Rust binary is unavailable
// (e.g. unsupported platform, OCTOCODE_SECURITY_FORCE_JS=1).
//
// SYNC NOTE: These patterns mirror src/security/patterns.rs in the Rust engine.
// Every time a pattern is added, removed, or changed in the Rust source it MUST
// be reflected here, and vice versa. There is no automated drift check — this
// is a manual invariant. Consider adding a parity test if patterns diverge.
import type { SensitiveDataPattern } from './types.js';

import { aiProviderPatterns } from './ai-providers.js';
import { analyticsModernPatterns } from './analytics.js';
import {
  authPatterns,
  codeConfigPatterns,
  cryptographicPatterns,
  privateKeyPatterns,
  genericSecretPatterns,
} from './auth-crypto.js';
import { awsPatterns } from './aws.js';
import { cloudProviderPatterns } from './cloudProviders.js';
import {
  slackPatterns,
  socialMediaPatterns,
  shippingLogisticsPatterns,
} from './communications.js';
import { databasePatterns } from './databases.js';
import { developerToolsPatterns } from './devTools.js';
import { mappingMonitoringPatterns } from './monitoring.js';
import {
  paymentProviderPatterns,
  ecommerceContentPatterns,
} from './payments-commerce.js';
import { versionControlPatterns } from './vcs.js';

export type { SensitiveDataPattern };

export const allRegexPatterns: SensitiveDataPattern[] = [
  ...aiProviderPatterns,
  ...analyticsModernPatterns,
  ...authPatterns,
  ...awsPatterns,
  ...cloudProviderPatterns,
  ...codeConfigPatterns,
  ...cryptographicPatterns,
  ...databasePatterns,
  ...developerToolsPatterns,
  ...ecommerceContentPatterns,
  ...genericSecretPatterns,
  ...mappingMonitoringPatterns,
  ...paymentProviderPatterns,
  ...privateKeyPatterns,
  ...shippingLogisticsPatterns,
  ...slackPatterns,
  ...socialMediaPatterns,
  ...versionControlPatterns,
];
