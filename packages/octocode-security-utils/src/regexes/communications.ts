import type { SensitiveDataPattern } from './types.js';

export const slackPatterns: SensitiveDataPattern[] = [
  {
    name: 'slackBotToken',
    description: 'Slack bot token',
    regex: /\bxoxb-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'slackUserToken',
    description: 'Slack user token',
    regex: /\bxoxp-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'slackWorkspaceToken',
    description: 'Slack workspace token',
    regex: /\bxoxa-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'slackRefreshToken',
    description: 'Slack refresh token',
    regex: /\bxoxr-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*\b/g,
    matchAccuracy: 'high',
  },
  // Slack Webhook URL
  {
    name: 'slackWebhookUrl',
    description: 'Slack incoming webhook URL',
    regex:
      /(?:https?:\/\/)?hooks\.slack\.com\/(?:services|workflows|triggers)\/[A-Za-z0-9+/]{43,56}/gi,
    matchAccuracy: 'high',
  },
  {
    name: 'slackWebhookUrlClassic',
    description: 'Slack classic incoming webhook URL',
    regex:
      /\bhttps:\/\/hooks\.slack\.com\/services\/[A-Z0-9]{8,12}\/[A-Z0-9]{8,12}\/[A-Za-z0-9]{20,32}\b/g,
    matchAccuracy: 'high',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
  // Slack App Token
  {
    name: 'slackAppToken',
    description: 'Slack app-level token',
    regex: /\bxapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+\b/gi,
    matchAccuracy: 'high',
  },
  // Slack Config Access Token
  {
    name: 'slackConfigAccessToken',
    description: 'Slack configuration access token',
    regex: /\bxoxe\.xox[bp]-\d-[A-Z0-9]{163,166}\b/gi,
    matchAccuracy: 'high',
  },
  // Sendbird Access Token
  {
    name: 'sendbirdAccessToken',
    description: 'Sendbird access token',
    regex:
      /\b['"]?(?:sendbird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{40}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // MessageBird API Token
  {
    name: 'messagebirdApiToken',
    description: 'MessageBird API token',
    regex:
      /\b['"]?(?:messagebird|message_bird|message-bird)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{25}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Mattermost Access Token
  {
    name: 'mattermostAccessToken',
    description: 'Mattermost access token',
    regex:
      /\b['"]?(?:mattermost)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{26}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Zendesk Secret Key
  {
    name: 'zendeskSecretKey',
    description: 'Zendesk secret key',
    regex:
      /\b['"]?(?:zendesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{40}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Freshdesk API Key
  {
    name: 'freshdeskApiKey',
    description: 'Freshdesk API key',
    regex:
      /\b['"]?(?:freshdesk)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{20}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Sendinblue (Brevo) API Token
  {
    name: 'sendinblueApiToken',
    description: 'Sendinblue (Brevo) API token',
    regex: /\bxkeysib-[a-f0-9]{64}-[a-z0-9]{16}\b/g,
    matchAccuracy: 'high',
  },

  // --- New Communication Patterns ---

  // Pusher App Secret
  {
    name: 'pusherAppSecret',
    description: 'Pusher app secret',
    regex:
      /\b['"]?(?:PUSHER|pusher)_?(?:APP|app)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{20}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Stream Chat/Activity API Secret
  {
    name: 'streamApiSecret',
    description: 'Stream (GetStream.io) API secret',
    regex:
      /\b['"]?(?:STREAM|stream|GETSTREAM)_?(?:API|api)?_?(?:SECRET|secret|KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{40,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Postmark Server Token
  {
    name: 'postmarkServerToken',
    description: 'Postmark server API token',
    regex: /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/g,
    matchAccuracy: 'medium',
    fileContext: /postmark/i,
  },
  // Vonage / Nexmo API Secret
  {
    name: 'vonageApiSecret',
    description: 'Vonage/Nexmo API secret',
    regex:
      /\b['"]?(?:VONAGE|NEXMO|vonage|nexmo)_?(?:API|api)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{16}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Customer.io API Key
  {
    name: 'customerIoApiKey',
    description: 'Customer.io API key',
    regex:
      /\b['"]?(?:CUSTOMERIO|customer_io|CUSTOMER_IO)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
];

export const socialMediaPatterns: SensitiveDataPattern[] = [
  {
    name: 'twitterBearerToken',
    description: 'Twitter/X Bearer token',
    regex: /\bAAAAAAAAAAAAAAAAAAAAA[a-zA-Z0-9%]{50,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'facebookAccessToken',
    description: 'Facebook/Meta access token',
    regex: /\bEAA[a-zA-Z0-9]{80,120}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'facebookPageAccessToken',
    description: 'Facebook/Meta page access token',
    regex: /\bEAAB[a-zA-Z0-9+/]{100,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'instagramAccessToken',
    description: 'Instagram access token',
    regex: /\bIGQV[a-zA-Z0-9_-]{100,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'discordSocialBotToken',
    description: 'Discord social bot token',
    regex: /\b[MN][A-Za-z\d]{23}\.[A-Za-z\d-_]{6}\.[A-Za-z\d-_]{27}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'discordSocialWebhookUrl',
    description: 'Discord social webhook URL',
    regex:
      /\bhttps:\/\/discord(?:app)?\.com\/api\/webhooks\/[0-9]{17,19}\/[A-Za-z0-9_-]{68}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'pinterestAccessToken',
    description: 'Pinterest access token',
    regex: /\bpina_[a-zA-Z0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  // LinkedIn API Token
  {
    name: 'linkedinApiToken',
    description: 'LinkedIn API token',
    regex:
      /\b['"]?(?:linkedin|linked_in|linked-in)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{14,16}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // YouTube API Key
  {
    name: 'youtubeApiKey',
    description: 'YouTube Data API key',
    regex:
      /\b['"]?(?:youtube)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?AIza[a-zA-Z0-9_-]{35}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // TikTok API Token
  {
    name: 'tiktokApiToken',
    description: 'TikTok API token',
    regex:
      /\b['"]?(?:tiktok)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{40,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
];

export const shippingLogisticsPatterns: SensitiveDataPattern[] = [
  // Shippo API Token
  {
    name: 'shippoApiToken',
    description: 'Shippo API token',
    regex: /\bshippo_(?:live|test)_[a-fA-F0-9]{40}\b/g,
    matchAccuracy: 'high',
  },
  // EasyPost API Token
  {
    name: 'easypostApiToken',
    description: 'EasyPost API token',
    regex: /\bEZAK[a-z0-9]{54}\b/gi,
    matchAccuracy: 'high',
  },
  // EasyPost Test API Token
  {
    name: 'easypostTestApiToken',
    description: 'EasyPost test API token',
    regex: /\bEZTK[a-z0-9]{54}\b/gi,
    matchAccuracy: 'high',
  },
  // Duffel API Token
  {
    name: 'duffelApiToken',
    description: 'Duffel travel API token',
    regex: /\bduffel_(?:test|live)_[a-z0-9_\-=]{43}\b/gi,
    matchAccuracy: 'high',
  },
  // Frame.io API Token
  {
    name: 'frameioApiToken',
    description: 'Frame.io API token',
    regex: /\bfio-u-[a-z0-9\-_=]{64}\b/gi,
    matchAccuracy: 'high',
  },
  // MaxMind License Key
  {
    name: 'maxmindLicenseKey',
    description: 'MaxMind license key',
    regex: /\b[A-Za-z0-9]{6}_[A-Za-z0-9]{29}_mmk\b/g,
    matchAccuracy: 'high',
  },
  // Asana Personal Access Token
  {
    name: 'asanaPersonalAccessToken',
    description: 'Asana personal access token',
    regex:
      /\b['"]?(?:asana)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[0-9]{16}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Monday.com API Token
  {
    name: 'mondayApiToken',
    description: 'Monday.com API token',
    regex:
      /\b['"]?(?:monday)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{100,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Trello API Key
  {
    name: 'trelloApiKey',
    description: 'Trello API key',
    regex:
      /\b['"]?(?:trello)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Jira API Token (legacy format)
  {
    name: 'jiraApiToken',
    description: 'Jira API token',
    regex:
      /\b['"]?(?:jira)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{24}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // SettleMint Application Access Token
  {
    name: 'settlemintApplicationAccessToken',
    description: 'SettleMint application access token',
    regex: /\bsm_aat_[a-zA-Z0-9]{16}\b/g,
    matchAccuracy: 'high',
  },
  // SettleMint Personal Access Token
  {
    name: 'settlemintPersonalAccessToken',
    description: 'SettleMint personal access token',
    regex: /\bsm_pat_[a-zA-Z0-9]{16}\b/g,
    matchAccuracy: 'high',
  },
  // SettleMint Service Access Token
  {
    name: 'settlemintServiceAccessToken',
    description: 'SettleMint service access token',
    regex: /\bsm_sat_[a-zA-Z0-9]{16}\b/g,
    matchAccuracy: 'high',
  },
];
