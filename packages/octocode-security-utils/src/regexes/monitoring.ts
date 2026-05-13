import type { SensitiveDataPattern } from './types.js';

export const mappingMonitoringPatterns: SensitiveDataPattern[] = [
  // Mapping Services
  {
    name: 'mapboxSecretToken',
    description: 'Mapbox secret access token',
    regex: /\bsk\.eyJ[a-zA-Z0-9._-]{87}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'mapboxPublicToken',
    description: 'Mapbox public access token',
    regex: /\bpk\.eyJ[a-zA-Z0-9._-]{80,}\b/g,
    matchAccuracy: 'high',
  },
  // Monitoring & Analytics
  {
    name: 'grafanaCloudApiKey',
    description: 'Grafana Cloud API key',
    regex: /\bglc_[a-zA-Z0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'newRelicApiKey',
    description: 'New Relic API key',
    regex: /\bNRAK-[A-Z0-9]{27}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'newRelicInsightKey',
    description: 'New Relic Insights query key',
    regex: /\bNRIK-[A-Z0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  // New Relic Browser API Token
  {
    name: 'newRelicBrowserApiToken',
    description: 'New Relic browser API token',
    regex: /\bNRJS-[a-f0-9]{19}\b/g,
    matchAccuracy: 'high',
  },
  // New Relic Insert Key
  {
    name: 'newRelicInsertKey',
    description: 'New Relic ingest insert key',
    regex: /\bNRII-[a-z0-9-]{32}\b/gi,
    matchAccuracy: 'high',
  },
  // Grafana API Key
  {
    name: 'grafanaApiKey',
    description: 'Grafana API key',
    regex: /\beyJrIjoi[A-Za-z0-9]{70,400}={0,3}\b/gi,
    matchAccuracy: 'high',
  },
  // Grafana Service Account Token
  {
    name: 'grafanaServiceAccountToken',
    description: 'Grafana service account token',
    regex: /\bglsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8}\b/g,
    matchAccuracy: 'high',
  },
  // Sentry Organization Token
  {
    name: 'sentryOrgToken',
    description: 'Sentry organization token',
    regex:
      /\bsntrys_eyJpYXQiO[a-zA-Z0-9+/]{10,200}(?:LCJyZWdpb25fdXJs|InJlZ2lvbl91cmwi|cmVnaW9uX3VybCI6)[a-zA-Z0-9+/]{10,200}={0,2}_[a-zA-Z0-9+/]{43}\b/g,
    matchAccuracy: 'high',
  },
  // Sentry User Token
  {
    name: 'sentryUserToken',
    description: 'Sentry user token',
    regex: /\bsntryu_[a-f0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  // SumoLogic Access ID
  {
    name: 'sumoLogicAccessId',
    description: 'SumoLogic access ID',
    regex:
      /\b['"]?(?:sumo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?su[a-zA-Z0-9]{12}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // Splunk API Token
  {
    name: 'splunkApiToken',
    description: 'Splunk HEC token',
    regex:
      /\b['"]?(?:splunk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // LogDNA / Mezmo API Key
  {
    name: 'logdnaApiKey',
    description: 'LogDNA/Mezmo API key',
    regex:
      /\b['"]?(?:logdna|mezmo)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Loggly Token
  {
    name: 'logglyToken',
    description: 'Loggly customer token',
    regex:
      /\b['"]?(?:loggly)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
];
