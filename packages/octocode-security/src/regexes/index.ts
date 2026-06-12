/**
 * allRegexPatterns — the canonical ordered list of all 304 secret-detection patterns.
 *
 * The patterns live here as TypeScript source (used by gen-patterns.mjs to generate
 * the Rust RegexSet). The hot path (sanitizeContent, maskSensitiveData) always runs
 * through the compiled Rust binary; this export exists for consumers that need to
 * inspect or iterate the pattern list.
 */
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

// Order must match the Rust patterns.rs — see scripts/gen-patterns.mjs
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
