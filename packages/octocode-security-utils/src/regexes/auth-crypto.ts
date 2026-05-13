import type { SensitiveDataPattern } from './types.js';

export const authPatterns: SensitiveDataPattern[] = [
  {
    name: 'jwtToken',
    description: 'JWT (JSON Web Token - 3-part)',
    regex:
      /\b(ey[a-zA-Z0-9]{17,}\.ey[a-zA-Z0-9/_-]{17,}\.(?:[a-zA-Z0-9/_-]{10,}={0,2})?)\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'sessionIds',
    description: 'Session IDs / Cookies',
    regex:
      /(?:JSESSIONID|PHPSESSID|ASP\.NET_SessionId|connect\.sid|session_id)=([a-zA-Z0-9%:._-]+)/gi,
    matchAccuracy: 'high',
  },
  {
    name: 'googleOauthToken',
    description: 'Google OAuth token',
    regex: /\bya29\.[a-zA-Z0-9_-]+\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'googleOauthRefreshToken',
    description: 'Google OAuth refresh token',
    regex:
      /\b['"]?(?:GOOGLE|google)?_?(?:OAUTH|oauth)?_?(?:REFRESH|refresh)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?(1\/\/0[a-zA-Z0-9._-]{40,})['"]?\b/g,
    matchAccuracy: 'high',
    fileContext: /(?:\.env|config|settings|secrets)/i,
  },
  {
    name: 'onePasswordSecretKey',
    description: '1Password secret key',
    regex:
      /\bA3-[A-Z0-9]{6}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}-[A-Z0-9]{5}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'onePasswordServiceAccountToken',
    description: '1Password service account token',
    regex: /\bops_eyJ[a-zA-Z0-9+/]+={0,2}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'jsonWebTokenEnhanced',
    description: 'JSON Web Token with enhanced detection',
    regex: /\bey[a-zA-Z0-9]+\.ey[a-zA-Z0-9/_-]+\.(?:[a-zA-Z0-9/_-]+={0,2})?\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'authressServiceClientAccessKey',
    description: 'Authress service client access key',
    regex:
      /\b(?:sc|ext|scauth|authress)_[a-z0-9]+\.[a-z0-9]+\.acc[_-][a-z0-9-]+\.[a-z0-9+/_=-]+\b/gi,
    matchAccuracy: 'high',
  },

  // --- New Auth Patterns ---

  // Auth0 Client Secret
  {
    name: 'auth0ClientSecret',
    description: 'Auth0 client secret',
    regex:
      /\b['"]?(?:AUTH0|auth0)_?(?:CLIENT|client)?_?(?:SECRET|secret)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{32,64}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Auth0 Management API Token
  {
    name: 'auth0ManagementToken',
    description: 'Auth0 Management API token',
    regex:
      /\b['"]?(?:AUTH0|auth0)_?(?:MANAGEMENT|management|MGMT)?_?(?:API)?_?(?:TOKEN|token)['"]?\s*(?::|=>|=)\s*['"]?eyJ[a-zA-Z0-9_-]{50,}['"]?\b/gi,
    matchAccuracy: 'high',
  },
  // Supertokens API Key
  {
    name: 'supertokensApiKey',
    description: 'SuperTokens API key',
    regex:
      /\b['"]?(?:SUPERTOKENS|supertokens)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9_-]{30,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },
  // Basic Auth header (base64-encoded credentials)
  {
    name: 'basicAuthHeader',
    description: 'Basic authentication header with credentials',
    regex: /\bBasic\s+[A-Za-z0-9+/]{20,}={0,2}\b/gi,
    matchAccuracy: 'medium',
  },
];

export const codeConfigPatterns: SensitiveDataPattern[] = [
  // Application Secrets
  {
    name: 'jwtSecrets',
    description: 'JWT secrets',
    regex: /\bjwt[_-]?secret\s*[:=]\s*['"][^'"]{16,}['"]\b/gi,
    matchAccuracy: 'high',
  },
  // Infrastructure & Deployment
  {
    name: 'kubernetesSecrets',
    description: 'Kubernetes secrets in YAML',
    regex:
      /\bkind:\s*["']?Secret["']?[\s\S]{0,2000}?\bdata:\s*[\s\S]{0,2000}?[a-zA-Z0-9_-]+:\s*[a-zA-Z0-9+/]{16,}={0,3}\b/gi,
    matchAccuracy: 'high',
    fileContext: /\.ya?ml$/i,
  },
  {
    name: 'dockerComposeSecrets',
    description: 'Docker Compose secrets',
    regex:
      /\b(?:MYSQL_ROOT_PASSWORD|POSTGRES_PASSWORD|REDIS_PASSWORD|MONGODB_PASSWORD)\s*[:=]\s*['"][^'"]{4,}['"]\b/gi,
    matchAccuracy: 'medium',
    fileContext: /docker-compose\.ya?ml$/i,
  },

  // Application Configuration
  {
    name: 'springBootSecrets',
    description: 'Spring Boot application secrets',
    regex:
      /\b(?:spring\.datasource\.password|spring\.security\.oauth2\.client\.registration\..*\.client-secret)\s*[:=]\s*['"][^'"]{4,}['"]\b/gi,
    matchAccuracy: 'medium',
    fileContext: /(?:application|bootstrap)(?:-\w+)?\.(?:properties|ya?ml)$/i,
  },
  {
    name: 'dotnetConnectionStrings',
    description: '.NET connection strings with credentials',
    regex:
      /\b(?:ConnectionStrings?|connectionString)\s*[:=]\s*['"][^'"]*(?:password|pwd)\s*=\s*[^;'"]{4,}[^'"]*['"]\b/gi,
    matchAccuracy: 'medium',
    fileContext: /(?:appsettings|web\.config).*\.(?:json|config)$/i,
  },

  // Generic High-Value Patterns
  {
    name: 'base64EncodedSecrets',
    description: 'Base64 encoded secrets in config',
    regex:
      /\b(?:secret|password|key|token)[_-]?(?:base64|encoded)?\s*[:=]\s*['"][A-Za-z0-9+/]{32,}={0,3}['"]\b/gi,
    matchAccuracy: 'medium',
  },
];

export const cryptographicPatterns: SensitiveDataPattern[] = [
  // Private Keys (PEM Format)
  {
    name: 'rsaPrivateKey',
    description: 'RSA private key',
    regex:
      /-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+)?PRIVATE\s+KEY-----/g,
    matchAccuracy: 'high',
  },
  {
    name: 'pkcs8PrivateKey',
    description: 'PKCS#8 private key',
    regex:
      /\b-----BEGIN (?:ENCRYPTED )?PRIVATE KEY-----\s*[\s\S]*?-----END (?:ENCRYPTED )?PRIVATE KEY-----\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'ecPrivateKey',
    description: 'Elliptic Curve private key',
    regex:
      /\b-----BEGIN EC PRIVATE KEY-----\s*[\s\S]*?-----END EC PRIVATE KEY-----\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'dsaPrivateKey',
    description: 'DSA private key',
    regex:
      /\b-----BEGIN DSA PRIVATE KEY-----\s*[\s\S]*?-----END DSA PRIVATE KEY-----\b/g,
    matchAccuracy: 'high',
  },

  // SSH Keys
  {
    name: 'opensshPrivateKey',
    description: 'OpenSSH private key',
    regex:
      /-----BEGIN\s+OPENSSH\s+PRIVATE\s+KEY-----[\s\S]*?-----END\s+OPENSSH\s+PRIVATE\s+KEY-----/g,
    matchAccuracy: 'high',
  },
  {
    name: 'sshPrivateKeyEncrypted',
    description: 'SSH private key (SSH2 encrypted format)',
    regex:
      /\b-----BEGIN SSH2 ENCRYPTED PRIVATE KEY-----\s*[\s\S]*?-----END SSH2 ENCRYPTED PRIVATE KEY-----\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'puttyPrivateKey',
    description: 'PuTTY private key file',
    regex: /\bPuTTY-User-Key-File-[23]:\s*[\s\S]*?Private-MAC:\b/g,
    matchAccuracy: 'high',
  },
  // PGP Keys
  {
    name: 'pgpPrivateKey',
    description: 'PGP private key block',
    regex:
      /\b-----BEGIN PGP PRIVATE KEY BLOCK-----\s*[\s\S]*?-----END PGP PRIVATE KEY BLOCK-----\b/g,
    matchAccuracy: 'high',
  },
  // Service-Specific Keys
  {
    name: 'firebaseServiceAccountPrivateKey',
    description: 'Firebase service account private key (JSON format)',
    regex:
      /\b"private_key":\s*"-----BEGIN PRIVATE KEY-----\\n[a-zA-Z0-9+/=\\n]+\\n-----END PRIVATE KEY-----"\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'openvpnClientPrivateKey',
    description: 'OpenVPN client private key',
    regex: /\b<key>\s*-----BEGIN[^<]*-----END[^<]*<\/key>\b/g,
    matchAccuracy: 'high',
  },

  // Cryptographic Parameters
  {
    name: 'dhParameters',
    description: 'Diffie-Hellman parameters',
    regex:
      /\b-----BEGIN DH PARAMETERS-----\s*[\s\S]*?-----END DH PARAMETERS-----\b/g,
    matchAccuracy: 'high',
  },

  // Modern Encryption Tools
  {
    name: 'ageSecretKey',
    description: 'Age encryption secret key',
    regex: /\bAGE-SECRET-KEY-1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{58}\b/g,
    matchAccuracy: 'high',
  },
  // Vault & Secrets Management
  {
    name: 'vaultBatchToken',
    description: 'HashiCorp Vault batch token',
    regex: /\bhvb\.[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'vaultServiceToken',
    description: 'HashiCorp Vault service token',
    regex: /\bhvs\.[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'vaultPeriodicToken',
    description: 'HashiCorp Vault periodic token',
    regex: /\bhvp\.[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },

  // Generic Cryptographic Patterns
  {
    name: 'base64PrivateKeyContent',
    description: 'Base64 encoded private key content',
    regex:
      /\b(?:private[_-]?key|secret[_-]?key)\s*[:=]\s*["'][A-Za-z0-9+/]{64,}={0,2}["']\b/gi,
    matchAccuracy: 'medium',
  },
  {
    name: 'hexEncodedKey',
    description: 'Hexadecimal encoded cryptographic key',
    regex: /\b(?:key|secret)\s*[:=]\s*["'][a-fA-F0-9]{32,}["']\b/gi,
    matchAccuracy: 'medium',
  },
];

export const privateKeyPatterns: SensitiveDataPattern[] = [
  // Private Key Detection - \s+ allows any whitespace between words (including multiple spaces)
  {
    name: 'privateKeyPem',
    description: 'Private key in PEM format (all key types)',
    regex:
      /-----BEGIN\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----[\s\S]*?-----END\s+(?:(?:RSA|DSA|EC|OPENSSH|ENCRYPTED)\s+)?PRIVATE\s+KEY(?:\s+BLOCK)?-----/g,
    matchAccuracy: 'high',
  },
  {
    name: 'pgpPrivateKeyBlock',
    description: 'PGP private key block (BEGIN to END)',
    regex:
      /-----BEGIN\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----[\s\S]*?-----END\s+PGP\s+PRIVATE\s+KEY\s+BLOCK-----/g,
    matchAccuracy: 'high',
  },
];

export const genericSecretPatterns: SensitiveDataPattern[] = [
  // URL-based Detection
  {
    name: 'credentialsInUrl',
    description: 'Credentials embedded in URL',
    regex: /\b[a-zA-Z]{3,10}:\/\/[^\\/\s:@]{3,20}:[^\\/\s:@]{3,20}@[^\s'"]+\b/g,
    matchAccuracy: 'high',
  },
  // Generic environment variable secrets
  {
    name: 'envVarSecrets',
    description: 'Environment variable secrets (KEY, SECRET, TOKEN, PASSWORD)',
    regex:
      /\b(?:\w+_)?(?:SECRET|secret|password|key|token|jwt_secret)(?:_\w+)?\s*=\s*["'][^"']{16,}["']/gi,
    matchAccuracy: 'medium',
  },
];
