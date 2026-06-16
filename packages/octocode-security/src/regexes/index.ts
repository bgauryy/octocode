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
