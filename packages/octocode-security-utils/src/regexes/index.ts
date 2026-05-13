/**
 * Sensitive data pattern detection regexes
 * Split into category modules for maintainability
 */

import type { SensitiveDataPattern } from './types.js';

import { aiProviderPatterns } from './ai-providers.js';
import { awsPatterns } from './aws.js';
import { analyticsModernPatterns } from './analytics.js';
import { cloudProviderPatterns } from './cloudProviders.js';
import { databasePatterns } from './databases.js';
import {
  authPatterns,
  codeConfigPatterns,
  cryptographicPatterns,
  privateKeyPatterns,
  genericSecretPatterns,
} from './auth-crypto.js';
import { developerToolsPatterns } from './devTools.js';
import { versionControlPatterns } from './vcs.js';
import { mappingMonitoringPatterns } from './monitoring.js';
import {
  paymentProviderPatterns,
  ecommerceContentPatterns,
} from './payments-commerce.js';
import {
  slackPatterns,
  socialMediaPatterns,
  shippingLogisticsPatterns,
} from './communications.js';

/**
 * Combined array of all sensitive data patterns
 * Use this for full secret detection across all pattern categories
 */
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
