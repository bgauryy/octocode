import type { SensitiveDataPattern } from './types.js';

export const analyticsModernPatterns: SensitiveDataPattern[] = [
  {
    name: 'vercelToken',
    description: 'Vercel API token (new prefixed formats: vcp/vci/vca/vcr/vck)',
    regex: /\b(?:vcp|vci|vca|vcr|vck)_[a-zA-Z0-9]{24,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'posthogApiKey',
    description: 'PostHog API key',
    regex: /\bphc_[a-zA-Z0-9_-]{39}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'posthogPersonalApiKey',
    description: 'PostHog personal API key',
    regex: /\bphx_[a-zA-Z0-9_-]{39}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'datadogApiKey',
    description: 'Datadog API and application keys (with context)',
    regex:
      /\bdatadog[\s\w]*(?:api|app)[\s\w]*key[\s:=]*["']?[a-fA-F0-9]{32,40}["']?/gi,
    matchAccuracy: 'medium',
  },
  {
    name: 'honeycombApiKey',
    description: 'Honeycomb API key',
    regex: /\bhcaik_[a-zA-Z0-9_-]{32,64}\b/g,
    matchAccuracy: 'high',
  },
];
