import type { SensitiveDataPattern } from './types.js';

export const cloudProviderPatterns: SensitiveDataPattern[] = [
  // Google Cloud Platform / Google AI (consolidated - covers GCP, Gemini, YouTube, Maps, etc.)
  {
    name: 'googleApiKey',
    description: 'Google API key (GCP, Gemini, Maps, YouTube, etc.)',
    regex: /\bAIza[a-zA-Z0-9_-]{30,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'googleOAuth2ClientId',
    description: 'Google OAuth2 client ID',
    regex: /\b[0-9]+-[a-z0-9]+\.apps\.googleusercontent\.com\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'googleOAuthClientSecret',
    description: 'Google OAuth client secret',
    regex: /\b"client_secret":\s*"[a-zA-Z0-9-_]{24}"\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'gcpServiceAccountEmail',
    description: 'GCP service account email',
    regex: /\b[a-z0-9-]+@[a-z0-9-]+\.iam\.gserviceaccount\.com\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'azureStorageConnectionString',
    description: 'Azure storage account connection string',
    regex:
      /\bDefaultEndpointsProtocol=https?;AccountName=[a-z0-9]+;AccountKey=[a-zA-Z0-9+/]+={0,2};EndpointSuffix=core\.windows\.net\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'azureSubscriptionId',
    description: 'Azure subscription ID',
    regex:
      /\b['"]?(?:AZURE|azure)?_?(?:SUBSCRIPTION|subscription)_?(?:ID|id)?['"]?\s*(?::|=>|=)\s*['"]?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}['"]?\b/gi,
    matchAccuracy: 'high',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
  {
    name: 'azureTenantDomain',
    description: 'Azure tenant domain (onmicrosoft.com)',
    regex:
      /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.onmicrosoft\.com\b/gi,
    matchAccuracy: 'medium',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
  {
    name: 'azureCosmosDbConnectionString',
    description: 'Azure Cosmos DB connection string',
    regex:
      /\bAccountEndpoint=https:\/\/[a-z0-9-]+\.documents\.azure\.com:443\/;AccountKey=[a-zA-Z0-9+/]+={0,2}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'azureServiceBusConnectionString',
    description: 'Azure Service Bus connection string',
    regex:
      /\bEndpoint=sb:\/\/[a-z0-9-]+\.servicebus\.windows\.net\/;SharedAccessKeyName=[a-zA-Z0-9]+;SharedAccessKey=[a-zA-Z0-9+/]+={0,2}\b/g,
    matchAccuracy: 'high',
  },

  // Dropbox
  {
    name: 'dropboxAccessToken',
    description: 'Dropbox access token',
    regex: /\bsl\.[a-zA-Z0-9_-]{64}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'dropboxAppKey',
    description: 'Dropbox app key',
    regex: /\b[a-z0-9]{15}\.(?:app|apps)\.dropbox\.com\b/g,
    matchAccuracy: 'high',
  },

  // Database Services
  {
    name: 'supabaseServiceKey',
    description: 'Supabase service role key',
    regex: /\bsbp_[a-f0-9]{40}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'planetScaleConnectionString',
    description: 'PlanetScale connection string',
    regex:
      /\bmysql:\/\/[a-zA-Z0-9_-]+:[a-zA-Z0-9_=-]+@[a-z0-9.-]+\.psdb\.cloud\/[a-zA-Z0-9_-]+\?sslaccept=strict\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'planetScaleToken',
    description: 'PlanetScale API token',
    regex: /\bpscale_tkn_[a-zA-Z0-9_-]{38,43}\b/g,
    matchAccuracy: 'high',
  },

  // Email Services
  {
    name: 'sendgridApiKey',
    description: 'SendGrid API key',
    regex: /\bSG\.[A-Za-z0-9_-]{20,22}\.[A-Za-z0-9_-]{43}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'mailgunApiKey',
    description: 'Mailgun API key',
    regex: /\bkey-[0-9a-z]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'mailchimpApiKey',
    description: 'MailChimp API key',
    regex: /\b[0-9a-f]{32}-us[0-9]{1,2}\b/g,
    matchAccuracy: 'high',
  },

  // Communication Platforms
  // NOTE: Discord bot token & webhook URL patterns are in communications.ts (discordSocialBotToken, discordSocialWebhookUrl)
  {
    name: 'telegramBotToken',
    description: 'Telegram bot token',
    regex: /\b[0-9]{8,10}:[A-Za-z0-9_-]{35}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'twilioApiKey',
    description: 'Twilio API key',
    regex: /\bSK[a-z0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'twilioAccountSid',
    description: 'Twilio account SID',
    regex: /\bAC[0-9a-fA-F]{32}\b/g,
    matchAccuracy: 'high',
  },

  // Package Managers & Registries
  {
    name: 'dockerHubToken',
    description: 'Docker Hub personal access token',
    regex: /\bdckr_pat_[a-zA-Z0-9_]{36}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'pypiApiToken',
    description: 'PyPI API token',
    regex: /\bpypi-[a-zA-Z0-9_-]{84}\b/g,
    matchAccuracy: 'high',
  },

  // Version Control & Development Tools
  {
    name: 'figmaToken',
    description: 'Figma personal access token',
    regex: /\bfigd_[a-zA-Z0-9_-]{43}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'renderToken',
    description: 'Render API token',
    regex: /\brnd_[a-zA-Z0-9_-]{43}\b/g,
    matchAccuracy: 'high',
  },
  // Business & Productivity Tools
  {
    name: 'airtablePersonalAccessToken',
    description: 'Airtable personal access token',
    regex: /\bpat[a-zA-Z0-9]{14}\.[a-zA-Z0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'typeformToken',
    description: 'Typeform API token',
    regex: /\btfp_[a-zA-Z0-9_-]{43}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'intercomAccessToken',
    description: 'Intercom access token',
    regex: /\bdG9rOi[a-zA-Z0-9+/]{46,48}={0,2}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'digitalOceanToken',
    description: 'DigitalOcean API token',
    regex: /\bdop_v1_[a-f0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  // DigitalOcean OAuth
  {
    name: 'digitalOceanOAuthToken',
    description: 'DigitalOcean OAuth access token',
    regex: /\bdoo_v1_[a-f0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  // DigitalOcean Refresh Token
  {
    name: 'digitalOceanRefreshToken',
    description: 'DigitalOcean OAuth refresh token',
    regex: /\bdor_v1_[a-f0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  // Cloudflare API Key
  {
    name: 'cloudflareApiKey',
    description: 'Cloudflare API key',
    regex:
      /\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9_-]{40}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Cloudflare Global API Key
  {
    name: 'cloudflareGlobalApiKey',
    description: 'Cloudflare Global API key',
    regex:
      /\b['"]?(?:cloudflare)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{37}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Cloudflare Origin CA Key
  {
    name: 'cloudflareOriginCaKey',
    description: 'Cloudflare Origin CA key',
    regex: /\bv1\.0-[a-f0-9]{24}-[a-f0-9]{146}\b/g,
    matchAccuracy: 'high',
  },
  // Fly.io Access Token
  {
    name: 'flyioAccessToken',
    description: 'Fly.io API access token',
    regex: /\bfo1_[\w-]{43}\b/g,
    matchAccuracy: 'high',
  },
  // Fly.io Machine Token
  {
    name: 'flyioMachineToken',
    description: 'Fly.io machine token',
    regex: /\bfm[12][ar]?_[a-zA-Z0-9+/]{100,}={0,3}\b/g,
    matchAccuracy: 'high',
  },
  // Doppler API Token
  {
    name: 'dopplerApiToken',
    description: 'Doppler API token',
    regex: /\bdp\.pt\.[a-z0-9]{43}\b/gi,
    matchAccuracy: 'high',
  },
  // Dynatrace API Token
  {
    name: 'dynatraceApiToken',
    description: 'Dynatrace API token',
    regex: /\bdt0c01\.[a-z0-9]{24}\.[a-z0-9]{64}\b/gi,
    matchAccuracy: 'high',
  },
  // Netlify Access Token
  {
    name: 'netlifyAccessToken',
    description: 'Netlify access token',
    regex:
      /\b['"]?(?:netlify)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9=_-]{40,46}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Scalingo API Token
  {
    name: 'scalingoApiToken',
    description: 'Scalingo API token',
    regex: /\btk-us-[\w-]{48}\b/g,
    matchAccuracy: 'high',
  },
  // Infracost API Token
  {
    name: 'infracostApiToken',
    description: 'Infracost API token',
    regex: /\bico-[a-zA-Z0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  // Harness API Key
  {
    name: 'harnessApiKey',
    description: 'Harness Access Token (PAT or SAT)',
    regex:
      /\b(?:pat|sat)\.[a-zA-Z0-9_-]{22}\.[a-zA-Z0-9]{24}\.[a-zA-Z0-9]{20}\b/g,
    matchAccuracy: 'high',
  },
  // Azure AD Client Secret
  {
    name: 'azureAdClientSecret',
    description: 'Azure AD client secret',
    regex:
      /(?:^|[\\'"` \s>=:(,)])([a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"` \s<),])/g,
    matchAccuracy: 'high',
  },
  // Heroku API Key v2
  {
    name: 'herokuApiKeyV2',
    description: 'Heroku API key (new format)',
    regex: /\bHRKU-AA[0-9a-zA-Z_-]{58}\b/g,
    matchAccuracy: 'high',
  },
  // Microsoft Teams Webhook
  {
    name: 'microsoftTeamsWebhook',
    description: 'Microsoft Teams incoming webhook URL',
    regex:
      /https:\/\/[a-z0-9]+\.webhook\.office\.com\/webhookb2\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}@[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}\/IncomingWebhook\/[a-z0-9]{32}\/[a-z0-9]{8}-(?:[a-z0-9]{4}-){3}[a-z0-9]{12}/gi,
    matchAccuracy: 'high',
  },
  // Okta Access Token
  {
    name: 'oktaAccessToken',
    description: 'Okta access token',
    regex:
      /\b['"]?(?:okta)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?00[\w=-]{40}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // OpenShift User Token
  {
    name: 'openshiftUserToken',
    description: 'OpenShift user token',
    regex: /\bsha256~[\w-]{43}\b/g,
    matchAccuracy: 'high',
  },

  // --- New Cloud Provider Patterns ---

  // Deno Deploy Access Token
  {
    name: 'denoDeployToken',
    description: 'Deno Deploy access token',
    regex: /\bddp_[a-zA-Z0-9]{40}\b/g,
    matchAccuracy: 'high',
  },
  // Resend API Key
  {
    name: 'resendApiKey',
    description: 'Resend email API key',
    regex: /\bre_[a-zA-Z0-9]{30,}\b/g,
    matchAccuracy: 'high',
  },
  // Azure OpenAI API Key
  {
    name: 'azureOpenaiApiKey',
    description: 'Azure OpenAI API key',
    regex:
      /\b['"]?(?:AZURE_OPENAI|azure_openai)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // Railway API Token
  {
    name: 'railwayApiToken',
    description: 'Railway API token',
    regex:
      /\b['"]?(?:RAILWAY|railway)_?(?:API|api)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // Convex Deploy Key
  {
    name: 'convexDeployKey',
    description: 'Convex deployment key',
    regex: /\b(?:prod|dev):[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]{40,}\b/g,
    matchAccuracy: 'medium',
  },
  // Upstash Kafka
  {
    name: 'upstashKafkaCredentials',
    description: 'Upstash Kafka REST credentials',
    regex:
      /\b['"]?(?:UPSTASH_KAFKA)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9=_-]{40,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Cloudflare Workers API Token (distinct prefix format)
  {
    name: 'cloudflareApiTokenPrefixed',
    description: 'Cloudflare Access team domain (not API token)',
    regex: /\b[A-Za-z0-9_-]{40}\.cloudflareaccess\.com\b/g,
    matchAccuracy: 'high',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
];
