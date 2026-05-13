import type { SensitiveDataPattern } from './types.js';

export const paymentProviderPatterns: SensitiveDataPattern[] = [
  {
    name: 'stripeSecretKey',
    description: 'Stripe secret key - live and test (sk_*, rk_*)',
    regex: /\b[rs]k_(?:live|test)_[a-zA-Z0-9]{20,247}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'stripeWebhookSecret',
    description: 'Stripe webhook signing secret',
    regex: /\bwhsec_[a-zA-Z0-9]{32,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'stripePublishableKey',
    description: 'Stripe publishable key (can indicate key pair presence)',
    regex: /\bpk_(?:live|test)_[a-zA-Z0-9]{20,247}\b/g,
    matchAccuracy: 'medium',
  },
  // PayPal
  {
    name: 'paypalAccessToken',
    description: 'PayPal access token',
    regex: /\bA21AA[a-zA-Z0-9_-]{50,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'paypalBraintreeAccessToken',
    description: 'PayPal Braintree access token',
    regex:
      /\baccess_token\$(?:production|sandbox)\$[0-9a-z]{16}\$[0-9a-f]{32}\b/g,
    matchAccuracy: 'high',
  },

  // Square (consolidated - all formats)
  {
    name: 'squareAccessToken',
    description: 'Square access token (all formats)',
    regex:
      /\b(?:EAAAE[A-Za-z0-9_-]{94,}|sq0[a-z]?atp-[0-9A-Za-z\-_]{22,26})\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'squareOauthSecret',
    description: 'Square OAuth secret',
    regex: /\bsq0csp-[0-9A-Za-z\-_]{43}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'squareApplicationId',
    description: 'Square application ID',
    regex: /\bsq0ids-[a-zA-Z0-9_-]{43}\b/g,
    matchAccuracy: 'high',
  },

  // Shopify
  {
    name: 'shopifyPrivateAppPassword',
    description: 'Shopify private app password',
    regex: /\bshppa_[a-fA-F0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'shopifyAccessToken',
    description: 'Shopify access token',
    regex: /\bshpat_[a-fA-F0-9]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'shopifyWebhookToken',
    description: 'Shopify webhook token',
    regex: /\bshpwh_[a-fA-F0-9]{32}\b/g,
    matchAccuracy: 'high',
  },

  // Other Payment Providers
  {
    name: 'adyenApiKey',
    description: 'Adyen API key',
    regex: /\bAQE[a-zA-Z0-9]{70,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'razorpayApiKey',
    description: 'Razorpay API key',
    regex: /\brzp_(?:test|live)_[a-zA-Z0-9]{14}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'flutterwaveKeys',
    description: 'Flutterwave API keys',
    regex: /\bFLW(?:PUBK|SECK)_(?:TEST|LIVE)-[a-h0-9]{32}-X\b/g,
    matchAccuracy: 'high',
  },

  // Cryptocurrency Exchanges
  // Coinbase
  {
    name: 'coinbaseAccessToken',
    description: 'Coinbase access token',
    regex:
      /\b['"]?(?:coinbase)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9_-]{64}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Kraken
  {
    name: 'krakenAccessToken',
    description: 'Kraken access token',
    regex:
      /\b['"]?(?:kraken)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9/=_+-]{80,90}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Kucoin
  {
    name: 'kucoinAccessToken',
    description: 'Kucoin access token',
    regex:
      /\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{24}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  {
    name: 'kucoinSecretKey',
    description: 'Kucoin secret key',
    regex:
      /\b['"]?(?:kucoin)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Bittrex
  {
    name: 'bittrexAccessKey',
    description: 'Bittrex access key',
    regex:
      /\b['"]?(?:bittrex)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[a-z0-9]{32}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Binance
  {
    name: 'binanceApiKey',
    description: 'Binance API key',
    regex:
      /\b['"]?(?:binance)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9]{64}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Bybit
  {
    name: 'bybitApiKey',
    description: 'Bybit API key',
    regex:
      /\b['"]?(?:bybit)(?:[\s\w.-]{0,20})['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9]{18,24}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // GoCardless
  {
    name: 'gocardlessApiToken',
    description: 'GoCardless API token',
    regex: /\blive_[a-z0-9\-_=]{40}\b/gi,
    matchAccuracy: 'high',
  },
  // Plaid
  {
    name: 'plaidApiToken',
    description: 'Plaid API token',
    regex:
      /\baccess-(?:sandbox|development|production)-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/g,
    matchAccuracy: 'high',
  },
  // Plaid Client ID
  {
    name: 'plaidClientId',
    description: 'Plaid client ID',
    regex:
      /\b['"]?(?:PLAID|plaid)_?(?:CLIENT|client)_?(?:ID|id)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{24}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Lemon Squeezy API Key
  {
    name: 'lemonSqueezyApiKey',
    description: 'Lemon Squeezy API key',
    regex:
      /\b['"]?(?:LEMONSQUEEZY|LEMON_SQUEEZY|lemonsqueezy)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{100,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Paddle API Key
  {
    name: 'paddleApiKey',
    description: 'Paddle API key',
    regex:
      /\b['"]?(?:PADDLE|paddle)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?pdl_(?:live|sdbx)_[a-zA-Z0-9]{40,}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // Mollie API Key
  {
    name: 'mollieApiKey',
    description: 'Mollie API key',
    regex: /\b(?:live|test)_[a-zA-Z0-9]{30,}\b/g,
    matchAccuracy: 'medium',
    fileContext: /mollie/i,
  },
];

export const ecommerceContentPatterns: SensitiveDataPattern[] = [
  // E-commerce Platforms
  {
    name: 'shopifyStorefrontAccessToken',
    description: 'Shopify storefront API access token',
    regex: /\bshpatf_[0-9a-f]{32}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'woocommerceConsumerKey',
    description: 'WooCommerce consumer key',
    regex: /\bck_[a-f0-9]{40}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'woocommerceConsumerSecret',
    description: 'WooCommerce consumer secret',
    regex: /\bcs_[a-f0-9]{40}\b/g,
    matchAccuracy: 'high',
  },

  // Content Management & CRM
  {
    name: 'contentfulAccessToken',
    description: 'Contentful access token',
    regex: /\bCFPAT-[0-9a-zA-Z]{20}\b/g,
    matchAccuracy: 'high',
  },

  // Email Marketing
  {
    name: 'mailchimpEcommerceApiKey',
    description: 'MailChimp E-commerce API key',
    regex: /\b[0-9a-f]{32}-[a-z]{2,3}[0-9]{1,2}\b/g,
    matchAccuracy: 'high',
  },
];
