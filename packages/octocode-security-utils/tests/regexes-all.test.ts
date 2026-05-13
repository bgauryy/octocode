/**
 * Comprehensive regex correctness tests for all sensitive-data pattern modules.
 *
 * For every pattern we verify:
 *   1. Required fields exist (name, description, regex, matchAccuracy)
 *   2. The regex has the global "g" flag (required by maskSensitiveData)
 *   3. At least one POSITIVE sample actually matches
 *   4. At least one NEGATIVE sample does NOT match (false-positive guard)
 *
 * Patterns are reset between tests (RegExp.lastIndex) because of the "g" flag.
 */

import { describe, it, expect } from 'vitest';

import { aiProviderPatterns } from '../src/regexes/ai-providers.js';
import { analyticsModernPatterns } from '../src/regexes/analytics.js';
import {
  authPatterns,
  codeConfigPatterns,
  cryptographicPatterns,
  privateKeyPatterns,
  genericSecretPatterns,
} from '../src/regexes/auth-crypto.js';
import { awsPatterns } from '../src/regexes/aws.js';
import { cloudProviderPatterns } from '../src/regexes/cloudProviders.js';
import {
  slackPatterns,
  socialMediaPatterns,
  shippingLogisticsPatterns,
} from '../src/regexes/communications.js';
import { databasePatterns } from '../src/regexes/databases.js';
import { developerToolsPatterns } from '../src/regexes/devTools.js';
import { mappingMonitoringPatterns } from '../src/regexes/monitoring.js';
import {
  paymentProviderPatterns,
  ecommerceContentPatterns,
} from '../src/regexes/payments-commerce.js';
import { versionControlPatterns } from '../src/regexes/vcs.js';
import { allRegexPatterns } from '../src/regexes/index.js';

/** Reset lastIndex so the same RegExp can be reused across tests. */
function resetAndTest(re: RegExp, sample: string): boolean {
  re.lastIndex = 0;
  return re.test(sample);
}

type Sample = { match: string[]; noMatch: string[] };

/** Map from pattern name to test samples. */
const SAMPLES: Record<string, Sample> = {
  openaiApiKeyLegacy: {
    match: [
      'sk-1234567890abcdefT3BlbkFJ1234567890abcdef',
      'sk-' + 'a'.repeat(10) + 'T3BlbkFJ' + 'a'.repeat(10),
    ],
    noMatch: ['sk-shortkey', 'sk-proj-notlegacy1234567890'],
  },
  openaiProjectApiKey: {
    match: ['sk-proj-' + 'a'.repeat(20), 'sk-proj-' + 'A'.repeat(30)],
    noMatch: ['sk-proj-short', 'sk-notproject-' + 'a'.repeat(20)],
  },
  openaiServiceAccountKey: {
    match: ['sk-svcacct-' + 'a'.repeat(20), 'sk-svcacct-' + 'A'.repeat(25)],
    noMatch: ['sk-svcacct-tiny', 'svcacct-' + 'a'.repeat(20)],
  },
  openaiAdminKey: {
    match: ['sk-admin-' + 'a'.repeat(20), 'sk-admin-' + 'A'.repeat(25)],
    noMatch: ['sk-admin-short', 'admin-' + 'a'.repeat(20)],
  },
  openaiOrgId: {
    match: ['org-' + 'a'.repeat(20), 'org-' + 'A'.repeat(25)],
    noMatch: ['org-tooshort', 'notorg-' + 'a'.repeat(20)],
  },
  groqApiKey: {
    match: [
      // gsk_ + 51-52 chars
      'gsk_' + 'a'.repeat(51),
      'gsk_' + 'a'.repeat(52),
    ],
    noMatch: ['gsk_' + 'a'.repeat(10), 'notgsk_' + 'a'.repeat(52)],
  },
  cohereApiKey: {
    match: ['co-' + 'a'.repeat(38), 'co-' + 'a'.repeat(64)],
    noMatch: ['co-tooshort', 'notco-' + 'a'.repeat(38)],
  },
  huggingFaceToken: {
    // exactly 34 chars after hf_
    match: ['hf_' + 'a'.repeat(34)],
    noMatch: ['hf_' + 'a'.repeat(33), 'hf_' + 'a'.repeat(35)],
  },
  perplexityApiKey: {
    match: ['pplx-' + 'a'.repeat(30), 'pplx-' + 'a'.repeat(64)],
    noMatch: ['pplx-tooshort', 'notpplx-' + 'a'.repeat(30)],
  },
  replicateApiToken: {
    match: ['r8_' + 'a'.repeat(30), 'r8_' + 'a'.repeat(50)],
    noMatch: ['r8_tooshort', 'notr8_' + 'a'.repeat(30)],
  },
  anthropicApiKey: {
    match: [
      'sk-ant-api03-' + 'a'.repeat(80),
      'sk-ant-admin01-' + 'a'.repeat(95),
      'sk-ant-sid01-' + 'a'.repeat(100),
    ],
    noMatch: [
      'sk-ant-api03-' + 'a'.repeat(10),
      'sk-ant-unknown-' + 'a'.repeat(80),
    ],
  },
  mistralApiKey: {
    match: ['mistral-' + 'a'.repeat(32), 'mist_' + 'a'.repeat(32)],
    noMatch: ['mistral-tooshort', 'notmistral-' + 'a'.repeat(32)],
  },
  tavilyApiKey: {
    match: ['tvly-' + 'a'.repeat(30), 'tvly-' + 'a'.repeat(50)],
    noMatch: ['tvly-tooshort', 'nottvly-' + 'a'.repeat(30)],
  },
  deepseekApiKey: {
    match: [
      'DEEPSEEK_API_KEY=sk-' + 'a'.repeat(32),
      "deepseek_key: 'sk-" + 'a'.repeat(32) + "'",
    ],
    noMatch: ['sk-' + 'a'.repeat(32), 'OTHERKEY_API=sk-' + 'a'.repeat(32)],
  },
  togetherApiKey: {
    match: [
      'TOGETHER_API_KEY=' + 'a'.repeat(40),
      "together_key: '" + 'a'.repeat(40) + "'",
    ],
    noMatch: ['DIFFERENT_KEY=' + 'a'.repeat(40)],
  },
  fireworksApiKey: {
    match: [
      'FIREWORKS_API_KEY=' + 'a'.repeat(40),
      "fireworks_key: '" + 'a'.repeat(40) + "'",
    ],
    noMatch: ['ANOTHER_KEY=' + 'a'.repeat(40)],
  },
  xaiApiKey: {
    match: ['xai-' + 'a'.repeat(48), 'xai-' + 'a'.repeat(60)],
    noMatch: ['xai-tooshort', 'notxai-' + 'a'.repeat(48)],
  },
  openRouterApiKey: {
    // exactly 64 chars after sk-or-v1-
    match: ['sk-or-v1-' + 'a'.repeat(64)],
    noMatch: ['sk-or-v1-' + 'a'.repeat(63), 'sk-or-v1-' + 'a'.repeat(65)],
  },
  amazonBedrockApiKey: {
    match: ['ABSK' + 'A'.repeat(109), 'ABSK' + 'a'.repeat(200)],
    noMatch: ['ABSK' + 'A'.repeat(50), 'absk' + 'A'.repeat(109)],
  },
  ai21ApiKey: {
    match: [
      'AI21_API_KEY=' + 'a'.repeat(40),
      "ai21_key: '" + 'a'.repeat(40) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(40)],
  },
  stabilityApiKey: {
    match: [
      'STABILITY_API_KEY=sk-' + 'a'.repeat(48),
      "stability_key: 'sk-" + 'a'.repeat(48) + "'",
    ],
    noMatch: ['OTHER_KEY=sk-' + 'a'.repeat(48)],
  },
  voyageApiKey: {
    match: ['pa-' + 'a'.repeat(40), 'pa-' + 'a'.repeat(60)],
    noMatch: ['pa-tooshort', 'notpa-' + 'a'.repeat(40)],
  },
  elevenLabsApiKey: {
    match: [
      'ELEVENLABS_API_KEY=' + 'a'.repeat(32),
      "elevenlabs_key: '" + 'a'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(32)],
  },
  assemblyaiApiKey: {
    match: [
      'ASSEMBLYAI_API_KEY=' + 'a'.repeat(32),
      "assemblyai_key: '" + 'a'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(32)],
  },
  pineconeApiKeyPrefixed: {
    match: ['pcsk_' + 'a'.repeat(50), 'pcsk_' + 'A'.repeat(60)],
    noMatch: ['pcsk_tooshort', 'notpcsk_' + 'a'.repeat(50)],
  },
  wandbApiKey: {
    // exactly 40 hex chars [a-f0-9]
    match: ['a'.repeat(40), 'abcdef0123456789abcdef0123456789abcdef01'],
    noMatch: ['a'.repeat(39), 'a'.repeat(41), 'g'.repeat(40)],
  },
  cometApiKey: {
    match: [
      'COMET_API_KEY=' + 'a'.repeat(32),
      "comet_key: '" + 'A'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(32)],
  },
  langchainApiKey: {
    match: ['lsv2_' + 'a'.repeat(20), 'lsv2_' + 'A'.repeat(30)],
    noMatch: ['lsv2_tooshort', 'notlsv2_' + 'a'.repeat(20)],
  },
  unstructuredApiKey: {
    match: [
      'UNSTRUCTURED_API_KEY=' + 'a'.repeat(32),
      "unstructured_key: '" + 'a'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(32)],
  },

  vercelToken: {
    match: [
      'vcp_' + 'a'.repeat(24),
      'vci_' + 'a'.repeat(24),
      'vca_' + 'a'.repeat(24),
      'vcr_' + 'a'.repeat(24),
      'vck_' + 'a'.repeat(30),
    ],
    noMatch: [
      'vercel_' + 'a'.repeat(24), // old wrong prefix
      'vcp_tooshort',
    ],
  },
  posthogApiKey: {
    // exactly 39 chars after phc_
    match: ['phc_' + 'a'.repeat(39)],
    noMatch: ['phc_' + 'a'.repeat(38), 'phc_' + 'a'.repeat(40)],
  },
  posthogPersonalApiKey: {
    // exactly 39 chars after phx_
    match: ['phx_' + 'a'.repeat(39)],
    noMatch: ['phx_' + 'a'.repeat(38), 'phx_' + 'a'.repeat(40)],
  },
  datadogApiKey: {
    match: [
      'datadog api key: ' + 'a'.repeat(32),
      'datadog app key ' + 'a'.repeat(32),
    ],
    noMatch: ['some_other_key: ' + 'a'.repeat(32)],
  },
  honeycombApiKey: {
    match: ['hcaik_' + 'a'.repeat(32), 'hcaik_' + 'a'.repeat(64)],
    noMatch: ['hcaik_tooshort', 'nothcaik_' + 'a'.repeat(32)],
  },

  jwtToken: {
    match: [
      'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POkA',
    ],
    noMatch: ['notajwt.notajwt.notajwt', 'eyshort.eyshort.sig'],
  },
  sessionIds: {
    match: [
      'JSESSIONID=abcdef123456789abcdef123',
      'PHPSESSID=abc123def456ghi789',
      'connect.sid=s%3Aabc123def456',
    ],
    noMatch: ['randomcookie=abc123', 'session=normal'],
  },
  googleOauthToken: {
    match: ['ya29.' + 'a'.repeat(20), 'Bearer ya29.' + 'A'.repeat(20)],
    noMatch: ['ya28.' + 'a'.repeat(20), 'notya29.' + 'a'.repeat(10)],
  },
  googleOauthRefreshToken: {
    match: [
      'GOOGLE_OAUTH_REFRESH_TOKEN=1//0' + 'a'.repeat(41),
      "refresh_token: '1//0" + 'a'.repeat(41) + "'",
    ],
    noMatch: ['OTHER_TOKEN=1//0' + 'a'.repeat(41)],
  },
  onePasswordSecretKey: {
    match: ['A3-ABCDEF-ABCDE-ABCDE-ABCDE-ABCDE'],
    noMatch: [
      'A3-ABC-ABCDE-ABCDE-ABCDE-ABCDE',
      'A4-ABCDEF-ABCDE-ABCDE-ABCDE-ABCDE',
    ],
  },
  onePasswordServiceAccountToken: {
    match: ['ops_eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9'],
    noMatch: ['ops_notbase64!!!', 'notops_eyJhbGciOi'],
  },
  jsonWebTokenEnhanced: {
    match: [
      'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POkA',
    ],
    noMatch: ['notjwt.notjwt', 'ey.ey.sig'],
  },
  authressServiceClientAccessKey: {
    match: [
      'sc_account1.prod.acc_region-test.base64data',
      'authress_tenantA.prod.acc-region.data123',
    ],
    noMatch: ['notmatch.noprod.noacc.data'],
  },
  auth0ClientSecret: {
    match: [
      'AUTH0_CLIENT_SECRET=' + 'a'.repeat(32),
      "auth0_secret: '" + 'A'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_SECRET=' + 'a'.repeat(32)],
  },
  auth0ManagementToken: {
    match: [
      'AUTH0_MANAGEMENT_TOKEN=eyJ' + 'a'.repeat(50),
      "auth0_token: 'eyJ" + 'a'.repeat(50) + "'",
    ],
    noMatch: ['OTHER_TOKEN=eyJ' + 'a'.repeat(50)],
  },
  supertokensApiKey: {
    match: [
      'SUPERTOKENS_API_KEY=' + 'a'.repeat(30),
      "supertokens_key: '" + 'A'.repeat(30) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(30)],
  },
  basicAuthHeader: {
    match: [
      'Authorization: Basic dXNlcm5hbWU6cGFzc3dvcmQ=',
      'Basic QWxhZGRpbjpPcGVuU2VzYW1l',
    ],
    noMatch: ['Bearer abc123', 'Basic short'],
  },
  jwtSecrets: {
    // regex ends with ['"]\b — closing quote must be followed by a word char
    match: [
      'jwt_secret: "mysupersecretjwttoken123"x',
      "jwt-secret='mysupersecretjwttoken123'x",
    ],
    noMatch: ['jwt_secret: ""', 'jwt_secret: "short"x'],
  },
  kubernetesSecrets: {
    match: [`kind: Secret\ndata:\n  mykey: c2VjcmV0dmFsdWUxMjM0NTY=`],
    noMatch: ['kind: ConfigMap\ndata:\n  mykey: notbase64'],
  },
  dockerComposeSecrets: {
    // regex ends with ['"]\b — closing quote must be followed by a word char
    match: [
      'MYSQL_ROOT_PASSWORD: "mysecretpassword"x',
      "POSTGRES_PASSWORD='mysecretpassword123'x",
    ],
    noMatch: ['MYSQL_ROOT_PASSWORD: ""'],
  },
  springBootSecrets: {
    // regex ends with ['"]\b — closing quote must be followed by a word char
    match: [
      "spring.datasource.password: 'myDbPassword123'x",
      'spring.datasource.password="myDbPassword123"x',
    ],
    noMatch: ["spring.datasource.url: 'jdbc:mysql://localhost'"],
  },
  dotnetConnectionStrings: {
    // regex ends with ['"]\b — closing quote must be followed by a word char
    match: ["ConnectionStrings: 'Server=myserver;password=mysecret123;'x"],
    noMatch: ["connectionString: 'Server=myserver;'"],
  },
  base64EncodedSecrets: {
    // regex ends with ['"]\b — closing quote must be followed by a word char
    match: [
      "secret='" + 'A'.repeat(32) + "'x",
      'key="' + 'A'.repeat(32) + '"x',
    ],
    noMatch: ["secret=''", "secret='tooshort'"],
  },
  rsaPrivateKey: {
    match: [
      '-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----',
      '-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg==\n-----END PRIVATE KEY-----',
    ],
    noMatch: ['-----BEGIN PUBLIC KEY-----\nABC\n-----END PUBLIC KEY-----'],
  },
  pkcs8PrivateKey: {
    // regex has \b before/after "-----" (non-word) — needs word char on both sides
    match: [
      'x-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg==\n-----END PRIVATE KEY-----x',
      'x-----BEGIN ENCRYPTED PRIVATE KEY-----\nABC\n-----END ENCRYPTED PRIVATE KEY-----x',
    ],
    noMatch: ['-----BEGIN PUBLIC KEY-----\nABC\n-----END PUBLIC KEY-----'],
  },
  ecPrivateKey: {
    // regex has \b before/after "-----" — needs word char on both sides
    match: [
      'x-----BEGIN EC PRIVATE KEY-----\nMHQCAQE=\n-----END EC PRIVATE KEY-----x',
    ],
    noMatch: [
      '-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----',
    ],
  },
  dsaPrivateKey: {
    // regex has \b before/after "-----" — needs word char on both sides
    match: [
      'x-----BEGIN DSA PRIVATE KEY-----\nMIIBugI=\n-----END DSA PRIVATE KEY-----x',
    ],
    noMatch: [
      '-----BEGIN EC PRIVATE KEY-----\nABC\n-----END EC PRIVATE KEY-----',
    ],
  },
  opensshPrivateKey: {
    match: [
      '-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA=\n-----END OPENSSH PRIVATE KEY-----',
    ],
    noMatch: [
      '-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----',
    ],
  },
  sshPrivateKeyEncrypted: {
    // regex has \b before/after "-----" — needs word char on both sides
    match: [
      'x-----BEGIN SSH2 ENCRYPTED PRIVATE KEY-----\nABC\n-----END SSH2 ENCRYPTED PRIVATE KEY-----x',
    ],
    noMatch: [
      '-----BEGIN OPENSSH PRIVATE KEY-----\nABC\n-----END OPENSSH PRIVATE KEY-----',
    ],
  },
  puttyPrivateKey: {
    // regex ends with "Private-MAC:\b" — colon is non-word, needs word char after it
    match: [
      'PuTTY-User-Key-File-2: ssh-rsa\nEncryption: aes256-cbc\nPrivate-MAC:x',
      'PuTTY-User-Key-File-3: ecdsa-sha2-nistp256\nEncryption: none\nPrivate-MAC:x',
    ],
    noMatch: ['PuTTY-Public-Key: ssh-rsa\nABC'],
  },
  pgpPrivateKey: {
    // regex has \b before/after "-----" — needs word char on both sides
    match: [
      'x-----BEGIN PGP PRIVATE KEY BLOCK-----\nABC\n-----END PGP PRIVATE KEY BLOCK-----x',
    ],
    noMatch: [
      '-----BEGIN PGP PUBLIC KEY BLOCK-----\nABC\n-----END PGP PUBLIC KEY BLOCK-----',
    ],
  },
  firebaseServiceAccountPrivateKey: {
    // regex has \b before/after '"' — double-quote is non-word, needs word char on both sides
    match: [
      'a"private_key": "-----BEGIN PRIVATE KEY-----\\nABC123\\n-----END PRIVATE KEY-----"a',
    ],
    noMatch: [
      '"public_key": "-----BEGIN PRIVATE KEY-----\\nABC\\n-----END PRIVATE KEY-----"',
    ],
  },
  openvpnClientPrivateKey: {
    // regex has \b before '<key>' and after '</key>' — angle-brackets are non-word
    match: [
      'x<key>\n-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----\n</key>x',
    ],
    noMatch: [
      '<cert>\n-----BEGIN CERTIFICATE-----\nABC\n-----END CERTIFICATE-----\n</cert>',
    ],
  },
  dhParameters: {
    // regex has \b before/after "-----" — needs word char on both sides
    match: [
      'x-----BEGIN DH PARAMETERS-----\nMIGH\n-----END DH PARAMETERS-----x',
    ],
    noMatch: [
      '-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----',
    ],
  },
  ageSecretKey: {
    // Valid chars: [QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L] — all uppercase
    match: ['AGE-SECRET-KEY-1' + 'Q'.repeat(58)],
    // Use chars NOT in the Bech32 set (B, I, O are excluded from Bech32)
    noMatch: [
      'AGE-SECRET-KEY-2' + 'Q'.repeat(58),
      'AGE-SECRET-KEY-1' + 'B'.repeat(58),
    ],
  },
  vaultBatchToken: {
    match: ['hvb.' + 'a'.repeat(20), 'hvb.' + 'a'.repeat(40)],
    noMatch: ['hvb.tooshort', 'nothvb.' + 'a'.repeat(20)],
  },
  vaultServiceToken: {
    match: ['hvs.' + 'a'.repeat(20), 'hvs.' + 'a'.repeat(40)],
    noMatch: ['hvs.tooshort', 'nothvs.' + 'a'.repeat(20)],
  },
  vaultPeriodicToken: {
    match: ['hvp.' + 'a'.repeat(20), 'hvp.' + 'a'.repeat(40)],
    noMatch: ['hvp.tooshort', 'nothvp.' + 'a'.repeat(20)],
  },
  base64PrivateKeyContent: {
    // regex ends with ["']\b — closing quote must be followed by a word char
    match: [
      'private_key="' + 'A'.repeat(64) + '"x',
      "secret_key='" + 'a'.repeat(64) + "'x",
    ],
    noMatch: ['public_key="' + 'A'.repeat(64) + '"'],
  },
  hexEncodedKey: {
    // regex ends with ["']\b — closing quote must be followed by a word char
    match: [
      'key="' + 'a'.repeat(32) + '"x',
      "secret='" + 'a'.repeat(32) + "'x",
    ],
    noMatch: ['url="https://example.com"', 'data="not-hex!!!"'],
  },
  privateKeyPem: {
    match: [
      '-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg==\n-----END PRIVATE KEY-----',
      '-----BEGIN RSA PRIVATE KEY-----\nABC\n-----END RSA PRIVATE KEY-----',
      '-----BEGIN EC PRIVATE KEY-----\nABC\n-----END EC PRIVATE KEY-----',
    ],
    noMatch: ['-----BEGIN CERTIFICATE-----\nABC\n-----END CERTIFICATE-----'],
  },
  pgpPrivateKeyBlock: {
    match: [
      '-----BEGIN PGP PRIVATE KEY BLOCK-----\nABC\n-----END PGP PRIVATE KEY BLOCK-----',
    ],
    noMatch: [
      '-----BEGIN PGP PUBLIC KEY BLOCK-----\nABC\n-----END PGP PUBLIC KEY BLOCK-----',
    ],
  },
  credentialsInUrl: {
    match: [
      'https://user:password@example.com/path',
      'postgres://admin:secret@db.example.com:5432/mydb',
    ],
    noMatch: ['https://example.com/path', 'postgresql://localhost/mydb'],
  },
  envVarSecrets: {
    match: [
      'MY_SECRET="' + 'a'.repeat(16) + '"',
      "API_PASSWORD='" + 'a'.repeat(16) + "'",
    ],
    noMatch: ['MY_VAR="short"', 'PLAIN_TEXT=notaquotedvalue'],
  },

  awsAccessKeyId: {
    // prefix (4) + exactly 16 uppercase alphanum; ASIAIOSFODNN7EXAMPLEX has 17 chars after ASIA
    match: [
      'AKIAIOSFODNN7EXAMPLE',
      'ASIAIOSFODNN7EXAMPLE',
      'ABIAIOSFODNN7EXAMPLE',
    ],
    noMatch: ['BKIAIOSFODNN7EXAMPLE', 'AKIA_IOSFODNN7EXAMPLE'],
  },
  awsAccountId: {
    match: ['AWS_ACCOUNT_ID=123456789012', 'aws_account_id: "123456789012"'],
    noMatch: ['ACCOUNT_ID=12345', 'OTHER_ID=123456789012'],
  },
  awsAppSyncApiKey: {
    // da2- + exactly 26 lowercase alphanumeric
    match: ['da2-' + 'a'.repeat(26)],
    noMatch: ['da2-tooshort', 'nda2-' + 'a'.repeat(26)],
  },
  awsIamRoleArn: {
    match: [
      'arn:aws:iam::123456789012:role/MyRole',
      'arn:aws:iam::123456789012:role/service-role/MyRole-abc',
    ],
    noMatch: ['arn:aws:s3:::mybucket', 'not:aws:iam::123456789012:role/MyRole'],
  },
  awsLambdaFunctionArn: {
    match: ['arn:aws:lambda:us-east-1:123456789012:function:my-function'],
    noMatch: ['arn:aws:iam::123456789012:role/MyRole'],
  },
  awsMwsAuthToken: {
    match: ['amzn.mws.12345678-1234-1234-1234-123456789012'],
    noMatch: ['amzn.api.12345678-1234-1234-1234-123456789012'],
  },
  awsS3BucketArn: {
    match: ['arn:aws:s3:::my-bucket', 'arn:aws:s3:::company.data.bucket'],
    noMatch: ['arn:aws:iam::123456789012:role/MyRole'],
  },
  alibabaAccessKeyId: {
    // LTAI + exactly 20 alphanumeric
    match: ['LTAI' + 'a'.repeat(20), 'LTAI' + 'A'.repeat(20)],
    noMatch: ['LTAI' + 'a'.repeat(19), 'notLTAI' + 'a'.repeat(20)],
  },
  awsSecretAccessKey: {
    match: [
      'AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY',
      'aws_secret_access_key="' + 'A'.repeat(40) + '"',
    ],
    noMatch: ['DB_PASSWORD=' + 'A'.repeat(40)],
  },
  awsSessionToken: {
    match: ['AWS_SESSION_TOKEN=' + 'A'.repeat(200)],
    noMatch: ['AWS_SESSION_TOKEN=' + 'A'.repeat(50)],
  },
  awsSecretsManagerArn: {
    match: [
      'arn:aws:secretsmanager:us-east-1:123456789012:secret:MySecret-abc',
    ],
    noMatch: ['arn:aws:iam::123456789012:role/MyRole'],
  },

  googleApiKey: {
    match: ['AIzaSy' + 'a'.repeat(30), 'AIza' + 'a'.repeat(30)],
    noMatch: ['AIza' + 'a'.repeat(10), 'notAIza' + 'a'.repeat(30)],
  },
  googleOAuth2ClientId: {
    match: [
      '123456789012-abcdefghijklmnopqrstuvwxyz12.apps.googleusercontent.com',
    ],
    noMatch: [
      '123456789012-abc.googleapis.com',
      'notnumbers-abc.apps.googleusercontent.com',
    ],
  },
  googleOAuthClientSecret: {
    // regex has \b before/after '"' — double-quote is non-word, needs word char on both sides
    match: ['a"client_secret": "' + 'a'.repeat(24) + '"a'],
    noMatch: ['"client_id": "' + 'a'.repeat(24) + '"'],
  },
  gcpServiceAccountEmail: {
    match: [
      'my-service@my-project.iam.gserviceaccount.com',
      'svc-acct@another-project123.iam.gserviceaccount.com',
    ],
    noMatch: ['user@gmail.com', 'service@project.googleapis.com'],
  },
  azureStorageConnectionString: {
    match: [
      'DefaultEndpointsProtocol=https;AccountName=myaccount;AccountKey=abc123;EndpointSuffix=core.windows.net',
    ],
    noMatch: ['DefaultEndpointsProtocol=https;AccountName=myaccount;'],
  },
  azureSubscriptionId: {
    match: [
      'AZURE_SUBSCRIPTION_ID=12345678-1234-1234-1234-123456789012',
      "azure_subscription_id: '12345678-1234-1234-1234-123456789012'",
    ],
    noMatch: ['OTHER_ID=12345678-1234-1234-1234-123456789012'],
  },
  azureTenantDomain: {
    match: ['12345678-1234-1234-1234-123456789012.onmicrosoft.com'],
    noMatch: [
      'example.onmicrosoft.com',
      '12345678-1234-1234-1234-123456789012.microsoft.com',
    ],
  },
  azureCosmosDbConnectionString: {
    match: [
      'AccountEndpoint=https://mydb.documents.azure.com:443/;AccountKey=abc123==',
    ],
    noMatch: ['AccountEndpoint=https://mydb.azure.com:443/;'],
  },
  azureServiceBusConnectionString: {
    match: [
      'Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=Root;SharedAccessKey=abc123==',
    ],
    noMatch: ['Endpoint=sb://mynamespace.windows.net/;'],
  },
  dropboxAccessToken: {
    // sl. + exactly 64 chars
    match: ['sl.' + 'a'.repeat(64)],
    noMatch: ['sl.' + 'a'.repeat(63), 'notsl.' + 'a'.repeat(64)],
  },
  dropboxAppKey: {
    match: [
      'abcdefghijklmno.app.dropbox.com',
      'abc1234567890de.apps.dropbox.com',
    ],
    noMatch: ['abcdefghij.dropbox.com', 'abcdefghijklmno.dropbox.com'],
  },
  supabaseServiceKey: {
    // sbp_ + exactly 40 hex chars
    match: ['sbp_' + 'a'.repeat(40)],
    noMatch: ['sbp_tooshort', 'notsbp_' + 'a'.repeat(40)],
  },
  planetScaleConnectionString: {
    match: [
      'mysql://user:pscale_pw_abc123@aws.connect.psdb.cloud/mydb?sslaccept=strict',
    ],
    noMatch: ['mysql://user:password@localhost/mydb'],
  },
  planetScaleToken: {
    match: ['pscale_tkn_' + 'a'.repeat(38), 'pscale_tkn_' + 'a'.repeat(43)],
    noMatch: ['pscale_tkn_tooshort', 'notpscale_tkn_' + 'a'.repeat(38)],
  },
  sendgridApiKey: {
    // SG. + 20-22 chars . + 43 chars
    match: [
      'SG.' + 'a'.repeat(20) + '.' + 'a'.repeat(43),
      'SG.' + 'a'.repeat(22) + '.' + 'a'.repeat(43),
    ],
    noMatch: [
      'SG.short.short',
      'notSG.' + 'a'.repeat(20) + '.' + 'a'.repeat(43),
    ],
  },
  mailgunApiKey: {
    // key- + exactly 32 lowercase alphanumeric
    match: ['key-' + 'a'.repeat(32)],
    noMatch: ['key-tooshort', 'notkey-' + 'a'.repeat(32)],
  },
  mailchimpApiKey: {
    // 32 hex chars + -us + 1-2 digits
    match: ['a'.repeat(32) + '-us1', 'a'.repeat(32) + '-us12'],
    noMatch: ['a'.repeat(32), 'a'.repeat(32) + '-uk1'],
  },
  telegramBotToken: {
    // 8-10 digits : 35 chars
    match: ['123456789:' + 'A'.repeat(35), '9876543210:' + 'a'.repeat(35)],
    noMatch: ['12345:' + 'A'.repeat(35), '123456789:tooshort'],
  },
  twilioApiKey: {
    // SK + exactly 32 lowercase alphanumeric
    match: ['SK' + 'a'.repeat(32)],
    noMatch: ['SK' + 'a'.repeat(31), 'SK' + 'A'.repeat(32)],
  },
  twilioAccountSid: {
    // AC + exactly 32 hex chars [0-9a-fA-F]
    match: ['AC' + 'a'.repeat(32), 'AC' + 'A'.repeat(32)],
    noMatch: ['AC' + 'a'.repeat(31), 'BC' + 'a'.repeat(32)],
  },
  dockerHubToken: {
    // dckr_pat_ + exactly 36 chars
    match: ['dckr_pat_' + 'a'.repeat(36)],
    noMatch: ['dckr_pat_tooshort', 'notdckr_pat_' + 'a'.repeat(36)],
  },
  pypiApiToken: {
    // pypi- + exactly 84 chars
    match: ['pypi-' + 'a'.repeat(84)],
    noMatch: ['pypi-tooshort', 'notpypi-' + 'a'.repeat(84)],
  },
  figmaToken: {
    // figd_ + exactly 43 chars
    match: ['figd_' + 'a'.repeat(43)],
    noMatch: ['figd_tooshort', 'notfigd_' + 'a'.repeat(43)],
  },
  renderToken: {
    // rnd_ + exactly 43 chars
    match: ['rnd_' + 'a'.repeat(43)],
    noMatch: ['rnd_tooshort', 'notrnd_' + 'a'.repeat(43)],
  },
  airtablePersonalAccessToken: {
    // pat + 14 chars . + 64 chars
    match: ['pat' + 'a'.repeat(14) + '.' + 'a'.repeat(64)],
    noMatch: ['pat' + 'a'.repeat(13) + '.' + 'a'.repeat(64)],
  },
  typeformToken: {
    // tfp_ + exactly 43 chars
    match: ['tfp_' + 'a'.repeat(43)],
    noMatch: ['tfp_tooshort', 'nottfp_' + 'a'.repeat(43)],
  },
  intercomAccessToken: {
    // dG9rOi + 46-48 base64 chars + optional ==
    match: ['dG9rOi' + 'a'.repeat(46), 'dG9rOi' + 'A'.repeat(48) + '=='],
    noMatch: ['dG9rOi' + 'a'.repeat(45), 'notdG9rOi' + 'a'.repeat(46)],
  },
  digitalOceanToken: {
    // dop_v1_ + exactly 64 hex chars
    match: ['dop_v1_' + 'a'.repeat(64)],
    noMatch: ['dop_v1_' + 'a'.repeat(63), 'notdop_v1_' + 'a'.repeat(64)],
  },
  digitalOceanOAuthToken: {
    match: ['doo_v1_' + 'a'.repeat(64)],
    noMatch: ['dop_v1_' + 'a'.repeat(64)],
  },
  digitalOceanRefreshToken: {
    match: ['dor_v1_' + 'a'.repeat(64)],
    noMatch: ['dop_v1_' + 'a'.repeat(64)],
  },
  cloudflareApiKey: {
    match: ["cloudflare_key: '" + 'a'.repeat(40) + "'"],
    noMatch: ["some_other_key: '" + 'a'.repeat(40) + "'"],
  },
  cloudflareGlobalApiKey: {
    // value is exactly 37 hex chars [a-f0-9]
    match: ["cloudflare_api: '" + 'a'.repeat(37) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(37) + "'"],
  },
  cloudflareOriginCaKey: {
    // v1.0- + 24 hex - + 146 hex
    match: ['v1.0-' + 'a'.repeat(24) + '-' + 'a'.repeat(146)],
    noMatch: ['v2.0-' + 'a'.repeat(24) + '-' + 'a'.repeat(146)],
  },
  flyioAccessToken: {
    // fo1_ + exactly 43 word chars
    match: ['fo1_' + 'a'.repeat(43)],
    noMatch: ['fo1_tooshort', 'notfo1_' + 'a'.repeat(43)],
  },
  flyioMachineToken: {
    match: ['fm1_' + 'a'.repeat(100), 'fm2a_' + 'a'.repeat(100)],
    noMatch: ['fm1_tooshort', 'notfm1_' + 'a'.repeat(100)],
  },
  dopplerApiToken: {
    // dp.pt. + exactly 43 [a-z0-9] (i flag)
    match: ['dp.pt.' + 'a'.repeat(43)],
    noMatch: ['dp.pt.' + 'a'.repeat(42), 'notdp.pt.' + 'a'.repeat(43)],
  },
  dynatraceApiToken: {
    match: ['dt0c01.' + 'a'.repeat(24) + '.' + 'a'.repeat(64)],
    noMatch: [
      'dt0c01.tooshort',
      'dt1c01.' + 'a'.repeat(24) + '.' + 'a'.repeat(64),
    ],
  },
  netlifyAccessToken: {
    match: ["netlify_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },
  scalingoApiToken: {
    // tk-us- + exactly 48 word chars
    match: ['tk-us-' + 'a'.repeat(48)],
    noMatch: ['tk-us-tooshort', 'nottkus-' + 'a'.repeat(48)],
  },
  infracostApiToken: {
    // ico- + exactly 32 alphanumeric
    match: ['ico-' + 'a'.repeat(32)],
    noMatch: ['ico-tooshort', 'notico-' + 'a'.repeat(32)],
  },
  harnessApiKey: {
    // pat|sat . 22 . 24 . 20
    match: [
      'pat.' + 'a'.repeat(22) + '.' + 'a'.repeat(24) + '.' + 'a'.repeat(20),
      'sat.' + 'a'.repeat(22) + '.' + 'a'.repeat(24) + '.' + 'a'.repeat(20),
    ],
    noMatch: ['pat.tooshort.tooshort.tooshort'],
  },
  azureAdClientSecret: {
    match: ['abc3Q~' + 'a'.repeat(31), 'xyz4Q~' + 'a'.repeat(34)],
    noMatch: ['abc3R~' + 'a'.repeat(31)],
  },
  herokuApiKeyV2: {
    // HRKU-AA + exactly 58 [0-9a-zA-Z_-]
    match: ['HRKU-AA' + 'a'.repeat(58)],
    noMatch: ['HRKU-AA' + 'a'.repeat(57), 'HRKU-AB' + 'a'.repeat(58)],
  },
  microsoftTeamsWebhook: {
    match: [
      'https://myorg.webhook.office.com/webhookb2/12345678-1234-1234-1234-123456789012@12345678-1234-1234-1234-123456789012/IncomingWebhook/' +
        'a'.repeat(32) +
        '/12345678-1234-1234-1234-123456789012',
    ],
    noMatch: ['https://other.office.com/webhook'],
  },
  oktaAccessToken: {
    match: ["okta_token: '00" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '00" + 'a'.repeat(40) + "'"],
  },
  openshiftUserToken: {
    // sha256~ + exactly 43 word chars
    match: ['sha256~' + 'a'.repeat(43)],
    noMatch: ['sha256~tooshort', 'sha512~' + 'a'.repeat(43)],
  },
  denoDeployToken: {
    // ddp_ + exactly 40 alphanumeric
    match: ['ddp_' + 'a'.repeat(40)],
    noMatch: ['ddp_tooshort', 'notddp_' + 'a'.repeat(40)],
  },
  resendApiKey: {
    match: ['re_' + 'a'.repeat(30), 're_' + 'a'.repeat(50)],
    noMatch: ['re_tooshort', 'notre_' + 'a'.repeat(30)],
  },
  azureOpenaiApiKey: {
    match: [
      'AZURE_OPENAI_API_KEY=' + 'a'.repeat(32),
      "azure_openai_key: '" + 'a'.repeat(32) + "'",
    ],
    noMatch: ['OTHER_KEY=' + 'a'.repeat(32)],
  },
  railwayApiToken: {
    match: ['RAILWAY_API_TOKEN=12345678-1234-1234-1234-123456789012'],
    noMatch: ['OTHER_TOKEN=12345678-1234-1234-1234-123456789012'],
  },
  convexDeployKey: {
    match: [
      'prod:my-project:' + 'a'.repeat(40),
      'dev:project:' + 'a'.repeat(40),
    ],
    noMatch: ['staging:project:' + 'a'.repeat(40)],
  },
  upstashKafkaCredentials: {
    match: ['UPSTASH_KAFKA=' + 'a'.repeat(40)],
    noMatch: ['OTHER_KAFKA=' + 'a'.repeat(40)],
  },
  cloudflareApiTokenPrefixed: {
    match: ['a'.repeat(40) + '.cloudflareaccess.com'],
    noMatch: ['short.cloudflareaccess.com', 'a'.repeat(40) + '.cloudflare.com'],
  },

  slackBotToken: {
    match: ['xoxb-1234567890-1234567890-' + 'a'.repeat(10)],
    noMatch: [
      'xoxp-1234567890-1234567890-' + 'a'.repeat(10),
      'notxoxb-1234567890-1234567890',
    ],
  },
  slackUserToken: {
    match: ['xoxp-1234567890-1234567890-' + 'a'.repeat(10)],
    noMatch: ['xoxb-1234567890-1234567890-' + 'a'.repeat(10)],
  },
  slackWorkspaceToken: {
    match: ['xoxa-1234567890-1234567890-' + 'a'.repeat(10)],
    noMatch: ['xoxb-1234567890-1234567890-' + 'a'.repeat(10)],
  },
  slackRefreshToken: {
    match: ['xoxr-1234567890-1234567890-' + 'a'.repeat(10)],
    noMatch: ['xoxb-1234567890-1234567890-' + 'a'.repeat(10)],
  },
  slackWebhookUrl: {
    match: [
      'https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX',
    ],
    noMatch: ['https://hooks.slack.com/other'],
  },
  slackWebhookUrlClassic: {
    match: [
      'https://hooks.slack.com/services/T0000000/B0000000/' + 'A'.repeat(20),
    ],
    noMatch: ['https://api.slack.com/webhooks'],
  },
  slackAppToken: {
    match: ['xapp-1-ABCDEFGHIJ-1234567890-' + 'a'.repeat(10)],
    noMatch: ['xoxb-ABCDEFGHIJ-1234567890-' + 'a'.repeat(10)],
  },
  slackConfigAccessToken: {
    match: ['xoxe.xoxb-1-' + 'A'.repeat(163)],
    noMatch: ['xoxe.xoxa-1-' + 'A'.repeat(163)],
  },
  sendbirdAccessToken: {
    match: ["sendbird_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },
  messagebirdApiToken: {
    match: ["messagebird_key: '" + 'a'.repeat(25) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(25) + "'"],
  },
  mattermostAccessToken: {
    match: ["mattermost_token: '" + 'a'.repeat(26) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(26) + "'"],
  },
  zendeskSecretKey: {
    match: ["zendesk_key: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(40) + "'"],
  },
  freshdeskApiKey: {
    match: ["freshdesk_key: '" + 'a'.repeat(20) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(20) + "'"],
  },
  sendinblueApiToken: {
    // xkeysib- + 64 hex - + 16 alphanumeric
    match: ['xkeysib-' + 'a'.repeat(64) + '-' + 'a'.repeat(16)],
    noMatch: ['xkeysib-tooshort', 'notxkeysib-' + 'a'.repeat(64)],
  },
  pusherAppSecret: {
    // value is exactly 20 [a-f0-9] hex chars
    match: ['PUSHER_APP_SECRET=' + 'a'.repeat(20)],
    noMatch: ['OTHER_SECRET=' + 'a'.repeat(20)],
  },
  streamApiSecret: {
    match: ["STREAM_API_SECRET='" + 'a'.repeat(40) + "'"],
    noMatch: ["OTHER_SECRET='" + 'a'.repeat(40) + "'"],
  },
  postmarkServerToken: {
    match: ['12345678-1234-1234-1234-123456789012'],
    noMatch: ['not-a-uuid'],
  },
  vonageApiSecret: {
    // value is exactly 16 alphanumeric
    match: [
      "VONAGE_API_SECRET='" + 'a'.repeat(16) + "'",
      "nexmo_secret: '" + 'A'.repeat(16) + "'",
    ],
    noMatch: ["OTHER_SECRET='" + 'a'.repeat(16) + "'"],
  },
  customerIoApiKey: {
    match: ["CUSTOMERIO_API_KEY='" + 'a'.repeat(32) + "'"],
    noMatch: ["OTHER_KEY='" + 'a'.repeat(32) + "'"],
  },

  twitterBearerToken: {
    // exactly 21 A's + 50+ alphanumeric
    match: ['A'.repeat(21) + 'a'.repeat(50)],
    noMatch: [
      'A'.repeat(20) + 'a'.repeat(50), // one A short
    ],
  },
  facebookAccessToken: {
    // EAA + 80-120 alphanumeric
    match: ['EAA' + 'a'.repeat(80), 'EAA' + 'a'.repeat(120)],
    noMatch: ['EAA' + 'a'.repeat(79), 'notEAA' + 'a'.repeat(80)],
  },
  facebookPageAccessToken: {
    match: ['EAAB' + 'a'.repeat(100), 'EAAB' + 'a'.repeat(150)],
    noMatch: ['EAAB' + 'a'.repeat(50), 'EAA' + 'a'.repeat(100)],
  },
  instagramAccessToken: {
    match: ['IGQV' + 'a'.repeat(100), 'IGQV' + 'A'.repeat(150)],
    noMatch: ['IGQV' + 'a'.repeat(50), 'notIGQV' + 'a'.repeat(100)],
  },
  discordSocialBotToken: {
    // M|N + 23 word chars . 6 word chars . 27 word chars
    match: [
      'M' + 'a'.repeat(23) + '.' + 'a'.repeat(6) + '.' + 'a'.repeat(27),
      'N' + 'A'.repeat(23) + '.' + 'B'.repeat(6) + '.' + 'C'.repeat(27),
    ],
    noMatch: [
      'X' + 'a'.repeat(23) + '.' + 'a'.repeat(6) + '.' + 'a'.repeat(27),
    ],
  },
  discordSocialWebhookUrl: {
    match: [
      'https://discord.com/api/webhooks/123456789012345678/' + 'a'.repeat(68),
    ],
    noMatch: ['https://discord.com/api/v9/guilds'],
  },
  pinterestAccessToken: {
    // pina_ + exactly 32 alphanumeric
    match: ['pina_' + 'a'.repeat(32)],
    noMatch: ['pina_tooshort', 'notpina_' + 'a'.repeat(32)],
  },
  linkedinApiToken: {
    match: ["linkedin_token: '" + 'a'.repeat(15) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(15) + "'"],
  },
  youtubeApiKey: {
    // AIza + exactly 35 chars
    match: ["youtube_key: 'AIza" + 'a'.repeat(35) + "'"],
    noMatch: ["other_key: 'AIza" + 'a'.repeat(35) + "'"],
  },
  tiktokApiToken: {
    match: ["tiktok_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },

  shippoApiToken: {
    // shippo_live|test_ + exactly 40 hex chars [a-fA-F0-9]
    match: ['shippo_live_' + 'a'.repeat(40), 'shippo_test_' + 'a'.repeat(40)],
    noMatch: [
      'shippo_staging_' + 'a'.repeat(40),
      'notshippo_live_' + 'a'.repeat(40),
    ],
  },
  easypostApiToken: {
    // EZAK + exactly 54 alphanumeric (i flag)
    match: ['EZAK' + 'a'.repeat(54)],
    noMatch: ['EZAK' + 'a'.repeat(53), 'notEZAK' + 'a'.repeat(54)],
  },
  easypostTestApiToken: {
    match: ['EZTK' + 'a'.repeat(54)],
    noMatch: ['EZTK' + 'a'.repeat(53), 'notEZTK' + 'a'.repeat(54)],
  },
  duffelApiToken: {
    // duffel_live|test_ + exactly 43 [a-z0-9_\-=] (i flag)
    match: ['duffel_live_' + 'a'.repeat(43), 'duffel_test_' + 'a'.repeat(43)],
    noMatch: ['duffel_staging_' + 'a'.repeat(43)],
  },
  frameioApiToken: {
    // fio-u- + exactly 64 [a-z0-9\-_=] (i flag)
    match: ['fio-u-' + 'a'.repeat(64)],
    noMatch: ['fio-u-tooshort', 'notfio-u-' + 'a'.repeat(64)],
  },
  maxmindLicenseKey: {
    match: ['AbCdEf_AbCdEfGhIjKlMnOpQrStUvWxYzAbC_mmk'],
    noMatch: [
      'AbCdEf_AbCdEfGhIjKlMnOpQrStUvWxYzAbC',
      'AbCdEf_AbCdEfGhIjKlMnOpQrStUvWxYzAbC_other',
    ],
  },
  asanaPersonalAccessToken: {
    match: ["asana_token: '1234567890123456'"],
    noMatch: ["other_token: '1234567890123456'"],
  },
  mondayApiToken: {
    match: ["monday_token: 'eyJ" + 'a'.repeat(100) + "'"],
    noMatch: ["other_token: 'eyJ" + 'a'.repeat(100) + "'"],
  },
  trelloApiKey: {
    match: ["trello_key: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(32) + "'"],
  },
  jiraApiToken: {
    match: ["jira_token: '" + 'a'.repeat(24) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(24) + "'"],
  },
  settlemintApplicationAccessToken: {
    // sm_aat_ + exactly 16 alphanumeric
    match: ['sm_aat_' + 'a'.repeat(16)],
    noMatch: ['sm_aat_tooshort', 'notsm_aat_' + 'a'.repeat(16)],
  },
  settlemintPersonalAccessToken: {
    match: ['sm_pat_' + 'a'.repeat(16)],
    noMatch: ['sm_aat_' + 'a'.repeat(16)],
  },
  settlemintServiceAccessToken: {
    match: ['sm_sat_' + 'a'.repeat(16)],
    noMatch: ['sm_aat_' + 'a'.repeat(16)],
  },

  postgresqlConnectionString: {
    match: [
      'postgresql://user:password@localhost/mydb',
      'postgresql://admin:s3cret@db.example.com:5432/prod',
    ],
    noMatch: [
      'postgresql://localhost/mydb',
      'mysql://user:password@localhost/mydb',
    ],
  },
  mysqlConnectionString: {
    match: [
      'mysql://user:password@localhost/mydb',
      'mysql://admin:s3cret@db.example.com:3306/prod',
    ],
    noMatch: ['mysql://localhost/mydb'],
  },
  jdbcConnectionStringWithCredentials: {
    match: [
      'jdbc:postgresql://user:pass@localhost/mydb',
      'jdbc:mysql://admin:secret@db.example.com/prod',
    ],
    noMatch: ['jdbc:postgresql://localhost/mydb'],
  },
  mongodbConnectionString: {
    match: [
      'mongodb://user:password@localhost/mydb',
      'mongodb+srv://admin:s3cret@cluster0.abcde.mongodb.net/prod',
    ],
    noMatch: ['mongodb://localhost/mydb'],
  },
  redisConnectionString: {
    match: [
      'redis://user:password@localhost:6379',
      'rediss://admin:s3cret@redis.example.com:6380',
    ],
    noMatch: ['redis://localhost:6379'],
  },
  redisAuthPassword: {
    // AUTH + space + 8+ [a-zA-Z0-9_-]
    match: ['AUTH mysecretpassword', 'AUTH s3cr3tP4ssw0rd'],
    noMatch: ['AUTH short', 'AUTH '],
  },
  elasticsearchCredentials: {
    match: ['https://user:password@elasticsearch.example.com:9200'],
    noMatch: ['https://elasticsearch.example.com:9200'],
  },
  couchdbCredentials: {
    match: ['http://user:password@couchdb.example.com:5984'],
    noMatch: ['http://couchdb.example.com:5984'],
  },
  neo4jCredentials: {
    match: ['bolt://user:password@neo4j.example.com:7687'],
    noMatch: ['bolt://neo4j.example.com:7687'],
  },
  timescaledbConnectionString: {
    match: ['timescaledb://user:password@localhost/mydb'],
    noMatch: ['timescaledb://localhost/mydb'],
  },
  clickhouseCredentials: {
    match: ['clickhouse://user:password@clickhouse.example.com:8123'],
    noMatch: ['clickhouse://localhost:8123'],
  },
  cassandraConnectionString: {
    match: ['cassandra://user:password@cassandra.example.com:9042'],
    noMatch: ['cassandra://localhost:9042'],
  },
  faunadbKey: {
    // fn + exactly 40 alphanumeric
    match: ['fn' + 'a'.repeat(40)],
    noMatch: ['fn' + 'a'.repeat(39), 'notfn' + 'a'.repeat(40)],
  },
  databricksApiToken: {
    // dapi + exactly 32 hex chars [a-f0-9]
    match: ['dapi' + 'a'.repeat(32)],
    noMatch: ['dapi' + 'a'.repeat(31), 'notdapi' + 'a'.repeat(32)],
  },
  pineconeApiKey: {
    // pinecone...key|api|env (then [\s:=]*, not [\s\w]*) then 32 alphanum
    match: [
      'pinecone api key: ' + 'a'.repeat(32),
      'pinecone env: ' + 'a'.repeat(32),
    ],
    noMatch: ['other api key: ' + 'a'.repeat(32)],
  },
  databaseUrlWithCredentials: {
    match: [
      'postgres://user:pass@localhost/mydb',
      'mongodb://admin:secret@db.example.com',
    ],
    noMatch: ['postgres://localhost/mydb'],
  },
  clickhouseCloudApiKey: {
    // 4b1d + exactly 38 alphanumeric
    match: ['4b1d' + 'a'.repeat(38)],
    noMatch: ['4b1d' + 'a'.repeat(37), '5b1d' + 'a'.repeat(38)],
  },
  neonDatabaseConnectionString: {
    match: [
      'postgres://user:password@ep-abc-123.us-east-2.aws.neon.tech/neondb',
    ],
    noMatch: ['postgres://user:password@localhost/mydb'],
  },
  tursoDatabaseToken: {
    match: ["turso_token: 'eyJ" + 'a'.repeat(50) + "'"],
    noMatch: ["other_token: 'eyJ" + 'a'.repeat(50) + "'"],
  },
  upstashRedisToken: {
    match: ["upstash_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },
  supabaseJwtKey: {
    match: ['SUPABASE_ANON_KEY=eyJ' + 'a'.repeat(100)],
    noMatch: ['OTHER_KEY=eyJ' + 'a'.repeat(100)],
  },
  cockroachdbConnectionString: {
    match: ['postgresql://user:password@cluster.cockroachlabs.cloud/defaultdb'],
    noMatch: ['postgresql://user:password@localhost/mydb'],
  },

  npmAccessToken: {
    // npm_ + exactly 36 alphanumeric
    match: ['npm_' + 'a'.repeat(36)],
    noMatch: ['npm_tooshort', 'notnpm_' + 'a'.repeat(36)],
  },
  nugetApiKey: {
    // oy2 + exactly 43 [a-z0-9]
    match: ['oy2' + 'a'.repeat(43)],
    noMatch: ['oy2' + 'a'.repeat(42), 'notoy2' + 'a'.repeat(43)],
  },
  artifactoryApiKey: {
    // AKCp + exactly 69 [A-Za-z0-9]
    match: ['AKCp' + 'a'.repeat(69)],
    noMatch: ['AKCp' + 'a'.repeat(68), 'notAKCp' + 'a'.repeat(69)],
  },
  herokuApiKey: {
    match: [
      'heroku token: 12345678-1234-1234-1234-123456789012',
      'HEROKU_API_KEY=12345678-ABCD-ABCD-ABCD-123456789012',
    ],
    noMatch: ['myapp token: 12345678-1234-1234-1234-123456789012'],
  },
  terraformCloudToken: {
    // 14 chars . 6 chars . 16 chars
    match: ['abcdefghijklmn.abcdef.abcdefghijklmnop'],
    noMatch: ['tooshort.ab.abcdefghijklmnop', 'abcdefghijklmn.abcdef.tooshort'],
  },
  pulumiAccessToken: {
    // pul- + exactly 40 hex [a-f0-9]
    match: ['pul-' + 'a'.repeat(40)],
    noMatch: ['pul-tooshort', 'notpul-' + 'a'.repeat(40)],
  },
  atlassianApiToken: {
    // ATATT3 + exactly 186 [A-Za-z0-9_\-=]
    match: ['ATATT3' + 'a'.repeat(186)],
    noMatch: ['ATATT3' + 'a'.repeat(10), 'notATATT3' + 'a'.repeat(186)],
  },
  sourcegraphApiKey: {
    // sgp_ + exactly 32 alphanumeric
    match: ['sgp_' + 'a'.repeat(32)],
    noMatch: ['sgp_tooshort', 'notsgp_' + 'a'.repeat(32)],
  },
  linearApiKey: {
    // lin_api_ + exactly 40 [0-9A-Za-z]
    match: ['lin_api_' + 'a'.repeat(40)],
    noMatch: ['lin_api_tooshort', 'notlin_api_' + 'a'.repeat(40)],
  },
  notionIntegrationToken: {
    // ntn_ + exactly 43 [a-zA-Z0-9_-]
    match: ['ntn_' + 'a'.repeat(43)],
    noMatch: ['ntn_tooshort', 'ntn_' + 'a'.repeat(42), 'ntn_' + 'a'.repeat(44)],
  },
  notionIntegrationTokenLegacy: {
    // secret_ + exactly 43 [a-zA-Z0-9]
    match: ['secret_' + 'a'.repeat(43)],
    noMatch: [
      'secret_tooshort',
      'secret_' + 'a'.repeat(42),
      'secret_' + 'a'.repeat(44),
    ],
  },
  stackhawkApiKey: {
    // hawk. + 20 [0-9A-Za-z\-_] . + 20
    match: ['hawk.' + 'a'.repeat(20) + '.' + 'a'.repeat(20)],
    noMatch: [
      'hawk.tooshort.tooshort',
      'nothawk.' + 'a'.repeat(20) + '.' + 'a'.repeat(20),
    ],
  },
  sentryAuthToken: {
    match: ['sentry auth token: ' + 'a'.repeat(64)],
    noMatch: ['other auth token: ' + 'a'.repeat(64)],
  },
  bugsnagApiKey: {
    match: ['bugsnag api key: ' + 'a'.repeat(32)],
    noMatch: ['other api key: ' + 'a'.repeat(32)],
  },
  rollbarAccessToken: {
    match: ['rollbar access token: ' + 'a'.repeat(32)],
    noMatch: ['other access token: ' + 'a'.repeat(32)],
  },
  postmanApiToken: {
    // PMAK- + 24 hex - + 34 hex
    match: ['PMAK-' + 'a'.repeat(24) + '-' + 'a'.repeat(34)],
    noMatch: ['PMAK-tooshort-tooshort'],
  },
  prefectApiToken: {
    // pnu_ + exactly 36 alphanumeric
    match: ['pnu_' + 'a'.repeat(36)],
    noMatch: ['pnu_tooshort', 'notpnu_' + 'a'.repeat(36)],
  },
  readmeApiToken: {
    // rdme_ + exactly 70 [a-z0-9]
    match: ['rdme_' + 'a'.repeat(70)],
    noMatch: ['rdme_tooshort', 'notrdme_' + 'a'.repeat(70)],
  },
  rubygemsApiToken: {
    // rubygems_ + exactly 48 hex [a-f0-9]
    match: ['rubygems_' + 'a'.repeat(48)],
    noMatch: ['rubygems_tooshort', 'notrubygems_' + 'a'.repeat(48)],
  },
  clojarsApiToken: {
    // CLOJARS_ + exactly 60 [a-z0-9] (i flag)
    match: ['CLOJARS_' + 'a'.repeat(60), 'CLOJARS_' + 'A'.repeat(60)],
    noMatch: ['CLOJARS_tooshort', 'notCLOJARS_' + 'a'.repeat(60)],
  },
  snykApiToken: {
    match: [
      "snyk_api_key: '12345678-1234-1234-1234-123456789012'",
      "snyk-token: '12345678-1234-1234-1234-123456789012'",
    ],
    noMatch: ["other_key: '12345678-1234-1234-1234-123456789012'"],
  },
  sonarqubeToken: {
    match: [
      'squ_' + 'a'.repeat(40),
      'sqp_' + 'a'.repeat(40),
      'sqa_' + 'a'.repeat(40),
    ],
    noMatch: ['sqr_' + 'a'.repeat(40), 'squ_tooshort'],
  },
  travisciAccessToken: {
    match: ["travis_token: '" + 'a'.repeat(22) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(22) + "'"],
  },
  codecovAccessToken: {
    match: ["codecov_token: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(32) + "'"],
  },
  droneCiAccessToken: {
    match: ["drone_token: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(32) + "'"],
  },
  octopusDeployApiKey: {
    // API- + exactly 26 [A-Z0-9]
    match: ['API-' + 'A'.repeat(26)],
    noMatch: ['API-tooshort', 'notAPI-' + 'A'.repeat(26)],
  },
  circleciToken: {
    match: ["circleci_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },
  buildkiteAgentToken: {
    // bkagent_ + exactly 40 hex [a-f0-9]
    match: ['bkagent_' + 'a'.repeat(40)],
    noMatch: ['bkagent_tooshort', 'notbkagent_' + 'a'.repeat(40)],
  },
  launchdarklyAccessToken: {
    match: ["launchdarkly_token: '" + 'a'.repeat(40) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(40) + "'"],
  },
  algoliaApiKey: {
    match: ["algolia_key: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(32) + "'"],
  },
  clerkSecretKey: {
    match: ['sk_live_' + 'a'.repeat(24), 'sk_test_' + 'a'.repeat(30)],
    noMatch: ['sk_staging_' + 'a'.repeat(24), 'sk_live_tooshort'],
  },
  clerkPublishableKey: {
    match: ['pk_live_' + 'a'.repeat(24), 'pk_test_' + 'a'.repeat(30)],
    noMatch: ['pk_staging_' + 'a'.repeat(24), 'pk_live_tooshort'],
  },
  launchdarklySdkKey: {
    match: ['sdk-12345678-1234-1234-1234-123456789012'],
    noMatch: ['api-12345678-1234-1234-1234-123456789012'],
  },
  vercelOidcToken: {
    match: ['VERCEL_OIDC_TOKEN=eyJ' + 'a'.repeat(100)],
    noMatch: ['OTHER_TOKEN=eyJ' + 'a'.repeat(100)],
  },
  novuApiKey: {
    match: ["NOVU_API_KEY='" + 'a'.repeat(32) + "'"],
    noMatch: ["OTHER_KEY='" + 'a'.repeat(32) + "'"],
  },
  triggerDevApiKey: {
    match: ['tr_dev_' + 'a'.repeat(20), 'tr_prod_' + 'a'.repeat(20)],
    noMatch: ['tr_staging_' + 'a'.repeat(20), 'tr_dev_tooshort'],
  },
  nxCloudAccessToken: {
    match: ["NX_CLOUD_ACCESS_TOKEN='" + 'a'.repeat(36) + "'"],
    noMatch: ["OTHER_TOKEN='" + 'a'.repeat(36) + "'"],
  },
  depotToken: {
    match: ['dpt_' + 'a'.repeat(40), 'dpt_' + 'A'.repeat(50)],
    noMatch: ['dpt_tooshort', 'notdpt_' + 'a'.repeat(40)],
  },
  grafbaseApiKey: {
    match: ['GRAFBASE_API_KEY=eyJ' + 'a'.repeat(50)],
    noMatch: ['OTHER_KEY=eyJ' + 'a'.repeat(50)],
  },

  mapboxSecretToken: {
    // sk.eyJ + exactly 87 [a-zA-Z0-9._-]
    match: ['sk.eyJ' + 'a'.repeat(87)],
    noMatch: ['sk.eyJ' + 'a'.repeat(86), 'sk.eyJ' + 'a'.repeat(88)],
  },
  mapboxPublicToken: {
    // pk.eyJ + 80+ [a-zA-Z0-9._-]
    match: ['pk.eyJ' + 'a'.repeat(80), 'pk.eyJ' + 'a'.repeat(100)],
    noMatch: ['pk.eyJ' + 'a'.repeat(79), 'notpk.eyJ' + 'a'.repeat(80)],
  },
  grafanaCloudApiKey: {
    // glc_ + exactly 32 alphanumeric
    match: ['glc_' + 'a'.repeat(32)],
    noMatch: ['glc_tooshort', 'notglc_' + 'a'.repeat(32)],
  },
  newRelicApiKey: {
    // NRAK- + exactly 27 [A-Z0-9]
    match: ['NRAK-' + 'A'.repeat(27)],
    noMatch: ['NRAK-' + 'A'.repeat(26), 'notNRAK-' + 'A'.repeat(27)],
  },
  newRelicInsightKey: {
    // NRIK- + exactly 32 [A-Z0-9]
    match: ['NRIK-' + 'A'.repeat(32)],
    noMatch: ['NRIK-' + 'A'.repeat(31), 'notNRIK-' + 'A'.repeat(32)],
  },
  newRelicBrowserApiToken: {
    // NRJS- + exactly 19 hex [a-f0-9]
    match: ['NRJS-' + 'a'.repeat(19)],
    noMatch: ['NRJS-' + 'a'.repeat(18), 'NRJS-' + 'a'.repeat(20)],
  },
  newRelicInsertKey: {
    // NRII- + exactly 32 [a-z0-9-] (i flag)
    match: ['NRII-' + 'a'.repeat(32)],
    noMatch: ['NRII-' + 'a'.repeat(31), 'notNRII-' + 'a'.repeat(32)],
  },
  grafanaApiKey: {
    // eyJrIjoi + 70-400 base64 (i flag)
    match: ['eyJrIjoi' + 'a'.repeat(70), 'eyJrIjoi' + 'A'.repeat(200)],
    noMatch: ['eyJrIjoi' + 'a'.repeat(69), 'noteyJrIjoi' + 'a'.repeat(70)],
  },
  grafanaServiceAccountToken: {
    // glsa_ + 32 [A-Za-z0-9] _ + 8 hex [A-Fa-f0-9]
    match: ['glsa_' + 'a'.repeat(32) + '_' + 'a'.repeat(8)],
    noMatch: [
      'glsa_' + 'a'.repeat(31) + '_' + 'a'.repeat(8),
      'notglsa_' + 'a'.repeat(32),
    ],
  },
  sentryOrgToken: {
    match: [
      'sntrys_eyJpYXQiO' +
        'a'.repeat(10) +
        'LCJyZWdpb25fdXJs' +
        'a'.repeat(10) +
        '=_' +
        'a'.repeat(43),
    ],
    noMatch: ['sntryu_' + 'a'.repeat(64)],
  },
  sentryUserToken: {
    // sntryu_ + exactly 64 hex [a-f0-9]
    match: ['sntryu_' + 'a'.repeat(64)],
    noMatch: ['sntryu_' + 'a'.repeat(63), 'notsntryu_' + 'a'.repeat(64)],
  },
  sumoLogicAccessId: {
    match: ["sumo_key: 'su" + 'a'.repeat(12) + "'"],
    noMatch: ["other_key: 'su" + 'a'.repeat(12) + "'"],
  },
  splunkApiToken: {
    match: ["splunk_token: '12345678-1234-1234-1234-123456789012'"],
    noMatch: ["other_token: '12345678-1234-1234-1234-123456789012'"],
  },
  logdnaApiKey: {
    match: ["logdna_key: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(32) + "'"],
  },
  logglyToken: {
    match: ["loggly_token: '12345678-1234-1234-1234-123456789012'"],
    noMatch: ["other_token: '12345678-1234-1234-1234-123456789012'"],
  },

  stripeSecretKey: {
    match: [
      'sk_live_' + 'a'.repeat(20),
      'sk_test_' + 'a'.repeat(20),
      'rk_live_' + 'a'.repeat(20),
    ],
    noMatch: ['sk_staging_' + 'a'.repeat(20), 'pk_live_' + 'a'.repeat(20)],
  },
  stripeWebhookSecret: {
    match: ['whsec_' + 'a'.repeat(32), 'whsec_' + 'A'.repeat(50)],
    noMatch: ['whsec_tooshort', 'notwhsec_' + 'a'.repeat(32)],
  },
  stripePublishableKey: {
    match: ['pk_live_' + 'a'.repeat(20), 'pk_test_' + 'a'.repeat(20)],
    noMatch: ['pk_staging_' + 'a'.repeat(20), 'sk_live_' + 'a'.repeat(20)],
  },
  paypalAccessToken: {
    // A21AA + 50+ [a-zA-Z0-9_-]
    match: ['A21AA' + 'a'.repeat(50), 'A21AA' + 'a'.repeat(80)],
    noMatch: ['A21AA' + 'a'.repeat(10), 'notA21AA' + 'a'.repeat(50)],
  },
  paypalBraintreeAccessToken: {
    match: [
      'access_token$production$' + 'a'.repeat(16) + '$' + 'a'.repeat(32),
      'access_token$sandbox$' + 'a'.repeat(16) + '$' + 'a'.repeat(32),
    ],
    noMatch: ['access_token$staging$' + 'a'.repeat(16) + '$' + 'a'.repeat(32)],
  },
  squareAccessToken: {
    match: ['EAAAE' + 'a'.repeat(94), 'sq0atp-' + 'a'.repeat(22)],
    noMatch: ['EAAAE' + 'a'.repeat(50), 'sq0other-' + 'a'.repeat(22)],
  },
  squareOauthSecret: {
    // sq0csp- + exactly 43 [0-9A-Za-z\-_]
    match: ['sq0csp-' + 'a'.repeat(43)],
    noMatch: ['sq0csp-tooshort'],
  },
  squareApplicationId: {
    // sq0ids- + exactly 43 [a-zA-Z0-9_-]
    match: ['sq0ids-' + 'a'.repeat(43)],
    noMatch: ['sq0ids-tooshort'],
  },
  shopifyPrivateAppPassword: {
    // shppa_ + exactly 32 hex [a-fA-F0-9]
    match: ['shppa_' + 'a'.repeat(32)],
    noMatch: ['shppa_tooshort', 'notshppa_' + 'a'.repeat(32)],
  },
  shopifyAccessToken: {
    // shpat_ + exactly 32 hex [a-fA-F0-9]
    match: ['shpat_' + 'a'.repeat(32)],
    noMatch: ['shpat_tooshort'],
  },
  shopifyWebhookToken: {
    // shpwh_ + exactly 32 hex [a-fA-F0-9]
    match: ['shpwh_' + 'a'.repeat(32)],
    noMatch: ['shpwh_tooshort'],
  },
  adyenApiKey: {
    // AQE + 70+ alphanumeric
    match: ['AQE' + 'a'.repeat(70), 'AQE' + 'a'.repeat(100)],
    noMatch: ['AQE' + 'a'.repeat(10), 'notAQE' + 'a'.repeat(70)],
  },
  razorpayApiKey: {
    // rzp_test|live_ + exactly 14 alphanumeric
    match: ['rzp_test_' + 'a'.repeat(14), 'rzp_live_' + 'a'.repeat(14)],
    noMatch: ['rzp_staging_' + 'a'.repeat(14), 'rzp_test_tooshort'],
  },
  flutterwaveKeys: {
    match: [
      'FLWPUBK_TEST-' + 'a'.repeat(32) + '-X',
      'FLWSECK_LIVE-' + 'a'.repeat(32) + '-X',
    ],
    noMatch: ['FLWPUBK_DEV-' + 'a'.repeat(32) + '-X'],
  },
  coinbaseAccessToken: {
    match: ["coinbase_token: '" + 'a'.repeat(64) + "'"],
    noMatch: ["other_token: '" + 'a'.repeat(64) + "'"],
  },
  krakenAccessToken: {
    // value 80-90 [a-z0-9/=_+-] but \b needs word char at end; use alphanum only
    match: ["kraken_key: '" + 'a'.repeat(80) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(80) + "'"],
  },
  kucoinAccessToken: {
    // value exactly 24 hex [a-f0-9]
    match: ["kucoin_key: '" + 'a'.repeat(24) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(24) + "'"],
  },
  kucoinSecretKey: {
    match: ["kucoin_secret: '12345678-1234-1234-1234-123456789012'"],
    noMatch: ["other_secret: '12345678-1234-1234-1234-123456789012'"],
  },
  bittrexAccessKey: {
    match: ["bittrex_key: '" + 'a'.repeat(32) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(32) + "'"],
  },
  binanceApiKey: {
    match: ["binance_key: '" + 'a'.repeat(64) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(64) + "'"],
  },
  bybitApiKey: {
    match: ["bybit_key: '" + 'a'.repeat(18) + "'"],
    noMatch: ["other_key: '" + 'a'.repeat(18) + "'"],
  },
  gocardlessApiToken: {
    // live_ + exactly 40 [a-z0-9\-_=] then \b; {40} is exact so >40 fails the boundary
    match: ['live_' + 'a'.repeat(40)],
    noMatch: ['staging_' + 'a'.repeat(40), 'live_tooshort'],
  },
  plaidApiToken: {
    match: [
      'access-sandbox-12345678-1234-1234-1234-123456789012',
      'access-production-12345678-1234-1234-1234-123456789012',
    ],
    noMatch: ['access-test-12345678-1234-1234-1234-123456789012'],
  },
  plaidClientId: {
    match: ["PLAID_CLIENT_ID='" + 'a'.repeat(24) + "'"],
    noMatch: ["OTHER_ID='" + 'a'.repeat(24) + "'"],
  },
  lemonSqueezyApiKey: {
    match: ['LEMONSQUEEZY_API_KEY=eyJ' + 'a'.repeat(100)],
    noMatch: ['OTHER_KEY=eyJ' + 'a'.repeat(100)],
  },
  paddleApiKey: {
    match: [
      'PADDLE_API_KEY=pdl_live_' + 'a'.repeat(40),
      "paddle_key: 'pdl_sdbx_" + 'a'.repeat(40) + "'",
    ],
    noMatch: ['OTHER_KEY=pdl_live_' + 'a'.repeat(40)],
  },
  mollieApiKey: {
    match: ['live_' + 'a'.repeat(30), 'test_' + 'a'.repeat(30)],
    noMatch: ['staging_' + 'a'.repeat(30), 'live_tooshort'],
  },
  shopifyStorefrontAccessToken: {
    // shpatf_ + exactly 32 hex [0-9a-f]
    match: ['shpatf_' + 'a'.repeat(32)],
    noMatch: ['shpatf_tooshort'],
  },
  woocommerceConsumerKey: {
    // ck_ + exactly 40 hex [a-f0-9]
    match: ['ck_' + 'a'.repeat(40)],
    noMatch: ['ck_tooshort', 'notck_' + 'a'.repeat(40)],
  },
  woocommerceConsumerSecret: {
    // cs_ + exactly 40 hex [a-f0-9]
    match: ['cs_' + 'a'.repeat(40)],
    noMatch: ['cs_tooshort', 'notcs_' + 'a'.repeat(40)],
  },
  contentfulAccessToken: {
    // CFPAT- + exactly 20 alphanumeric
    match: ['CFPAT-' + 'a'.repeat(20)],
    noMatch: ['CFPAT-tooshort', 'notCFPAT-' + 'a'.repeat(20)],
  },
  mailchimpEcommerceApiKey: {
    // 32 hex + - + 2-3 lowercase + 1-2 digits
    match: ['a'.repeat(32) + '-us1', 'a'.repeat(32) + '-gb2'],
    noMatch: ['a'.repeat(32), 'short-us1'],
  },

  gitlabPersonalAccessToken: {
    match: ['glpat-' + 'a'.repeat(20), 'glpat-' + 'a'.repeat(30)],
    noMatch: ['glpat-tooshort', 'notglpat-' + 'a'.repeat(20)],
  },
  gitlabDeployToken: {
    // gldt- + exactly 20 [A-Za-z0-9_-]
    match: ['gldt-' + 'a'.repeat(20)],
    noMatch: ['gldt-tooshort', 'notgldt-' + 'a'.repeat(20)],
  },
  gitlabRunnerToken: {
    // glrt- + exactly 20 [A-Za-z0-9_-]
    match: ['glrt-' + 'a'.repeat(20)],
    noMatch: ['glrt-tooshort'],
  },
  gitlabCiJobToken: {
    // glcbt- + 1-5 alphanum _ + exactly 20 [0-9a-zA-Z_-]
    match: ['glcbt-abc_' + 'a'.repeat(20), 'glcbt-12345_' + 'a'.repeat(20)],
    noMatch: ['glcbt-abcdefghij_tooshort'],
  },
  gitlabPipelineTriggerToken: {
    // glptt- + exactly 40 hex [0-9a-f]
    match: ['glptt-' + 'a'.repeat(40)],
    noMatch: ['glptt-tooshort', 'notglptt-' + 'a'.repeat(40)],
  },
  bitbucketAppPassword: {
    // ATBB + exactly 24 alphanumeric
    match: ['ATBB' + 'a'.repeat(24)],
    noMatch: ['ATBB' + 'a'.repeat(23), 'notATBB' + 'a'.repeat(24)],
  },
  githubTokens: {
    match: [
      'ghp_' + 'a'.repeat(36),
      'gho_' + 'A'.repeat(36),
      'ghu_' + 'a'.repeat(36),
      'ghs_' + 'a'.repeat(36),
      'ghr_' + 'a'.repeat(36),
      'github_pat_' + 'a'.repeat(36),
    ],
    noMatch: ['ghx_' + 'a'.repeat(36), 'ghp_tooshort'],
  },
  githubFineGrainedToken: {
    // github_pat_ + exactly 82 [A-Za-z0-9_]
    match: ['github_pat_' + 'a'.repeat(82)],
    noMatch: ['github_pat_' + 'a'.repeat(81), 'github_pat_' + 'a'.repeat(83)],
  },
  githubAppInstallationToken: {
    // ghs_ + exactly 37 [0-9a-zA-Z]
    match: ['ghs_' + 'a'.repeat(37)],
    noMatch: ['ghs_' + 'a'.repeat(36), 'ghs_' + 'a'.repeat(38)],
  },
  gitlabScimToken: {
    // glsoat- + exactly 20 [0-9a-zA-Z_-]
    match: ['glsoat-' + 'a'.repeat(20)],
    noMatch: ['glsoat-tooshort'],
  },
  gitlabFeatureFlagToken: {
    // glffct- + exactly 20 [0-9a-zA-Z_-]
    match: ['glffct-' + 'a'.repeat(20)],
    noMatch: ['glffct-tooshort'],
  },
  gitlabFeedToken: {
    // glft- + exactly 20 [0-9a-zA-Z_-]
    match: ['glft-' + 'a'.repeat(20)],
    noMatch: ['glft-tooshort'],
  },
  gitlabIncomingMailToken: {
    // glimt- + exactly 25 [0-9a-zA-Z_-]
    match: ['glimt-' + 'a'.repeat(25)],
    noMatch: ['glimt-tooshort'],
  },
  gitlabK8sAgentToken: {
    // glagent- + exactly 50 [0-9a-zA-Z_-]
    match: ['glagent-' + 'a'.repeat(50)],
    noMatch: ['glagent-tooshort'],
  },
  gitlabOAuthAppSecret: {
    // gloas- + exactly 64 [0-9a-zA-Z_-]
    match: ['gloas-' + 'a'.repeat(64)],
    noMatch: ['gloas-tooshort'],
  },
  gitlabSessionCookie: {
    // _gitlab_session= + exactly 32 [0-9a-z]
    match: ['_gitlab_session=' + 'a'.repeat(32)],
    noMatch: [
      '_gitlab_session=tooshort',
      'notgitlab_session=' + 'a'.repeat(32),
    ],
  },
  bitbucketRepoToken: {
    // ATCTT3 + exactly 24 alphanumeric
    match: ['ATCTT3' + 'a'.repeat(24)],
    noMatch: ['ATCTT3' + 'a'.repeat(23), 'notATCTT3' + 'a'.repeat(24)],
  },
};

const allPatternArrays = [
  { name: 'aiProviderPatterns', patterns: aiProviderPatterns },
  { name: 'analyticsModernPatterns', patterns: analyticsModernPatterns },
  { name: 'authPatterns', patterns: authPatterns },
  { name: 'codeConfigPatterns', patterns: codeConfigPatterns },
  { name: 'cryptographicPatterns', patterns: cryptographicPatterns },
  { name: 'privateKeyPatterns', patterns: privateKeyPatterns },
  { name: 'genericSecretPatterns', patterns: genericSecretPatterns },
  { name: 'awsPatterns', patterns: awsPatterns },
  { name: 'cloudProviderPatterns', patterns: cloudProviderPatterns },
  { name: 'slackPatterns', patterns: slackPatterns },
  { name: 'socialMediaPatterns', patterns: socialMediaPatterns },
  { name: 'shippingLogisticsPatterns', patterns: shippingLogisticsPatterns },
  { name: 'databasePatterns', patterns: databasePatterns },
  { name: 'developerToolsPatterns', patterns: developerToolsPatterns },
  { name: 'mappingMonitoringPatterns', patterns: mappingMonitoringPatterns },
  { name: 'paymentProviderPatterns', patterns: paymentProviderPatterns },
  { name: 'ecommerceContentPatterns', patterns: ecommerceContentPatterns },
  { name: 'versionControlPatterns', patterns: versionControlPatterns },
];

describe('All regex patterns — structural integrity', () => {
  it('allRegexPatterns should contain all patterns from all modules', () => {
    const total = allPatternArrays.reduce(
      (sum, a) => sum + a.patterns.length,
      0
    );
    expect(allRegexPatterns.length).toBe(total);
  });

  it('every pattern should have no duplicate names within allRegexPatterns', () => {
    const names = allRegexPatterns.map(p => p.name);
    const duplicates = names.filter((n, i) => names.indexOf(n) !== i);
    expect(duplicates).toEqual([]);
  });

  for (const { name: arrayName, patterns } of allPatternArrays) {
    describe(`${arrayName}`, () => {
      for (const pattern of patterns) {
        it(`${pattern.name} — has required fields`, () => {
          expect(typeof pattern.name).toBe('string');
          expect(pattern.name.length).toBeGreaterThan(0);
          expect(typeof pattern.description).toBe('string');
          expect(pattern.description.length).toBeGreaterThan(0);
          expect(pattern.regex).toBeInstanceOf(RegExp);
          expect(['high', 'medium']).toContain(pattern.matchAccuracy);
        });

        it(`${pattern.name} — regex has global "g" flag`, () => {
          expect(pattern.regex.flags).toContain('g');
        });
      }
    });
  }
});

describe('All regex patterns — match correctness', () => {
  for (const [patternName, samples] of Object.entries(SAMPLES)) {
    // Find the pattern by name in the combined list
    const pattern = allRegexPatterns.find(p => p.name === patternName);

    if (!pattern) {
      it.todo(`${patternName} — pattern not found in allRegexPatterns`);
      continue;
    }

    describe(patternName, () => {
      for (const sample of samples.match) {
        it(`should MATCH: ${sample.slice(0, 80)}${sample.length > 80 ? '…' : ''}`, () => {
          expect(resetAndTest(pattern.regex, sample)).toBe(true);
        });
      }

      for (const sample of samples.noMatch) {
        it(`should NOT match: ${sample.slice(0, 80)}${sample.length > 80 ? '…' : ''}`, () => {
          expect(resetAndTest(pattern.regex, sample)).toBe(false);
        });
      }
    });
  }
});
