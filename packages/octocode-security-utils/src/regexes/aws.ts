import type { SensitiveDataPattern } from './types.js';

export const awsPatterns: SensitiveDataPattern[] = [
  {
    name: 'awsAccessKeyId',
    description: 'AWS access key ID',
    regex: /\b((?:AKIA|ABIA|ACCA|ASIA)[A-Z0-9]{16})\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsAccountId',
    description: 'AWS account ID',
    regex:
      /\b['"]?(?:AWS|aws|Aws)?_?(?:ACCOUNT|account|Account)_?(?:ID|id|Id)?['"]?\s*(?::|=>|=)\s*['"]?[0-9]{12}['"]?\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsAppSyncApiKey',
    description: 'AWS AppSync GraphQL API key',
    regex: /\bda2-[a-z0-9]{26}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsIamRoleArn',
    description: 'AWS IAM role ARN',
    regex: /\barn:aws:iam::[0-9]{12}:role\/[a-zA-Z0-9_+=,.@-]+\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsLambdaFunctionArn',
    description: 'AWS Lambda function ARN',
    regex: /\barn:aws:lambda:[a-z0-9-]+:[0-9]{12}:function:[a-zA-Z0-9_-]+\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsMwsAuthToken',
    description: 'AWS MWS authentication token',
    regex:
      /\bamzn\.mws\.[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsS3BucketArn',
    description: 'AWS S3 bucket ARN',
    regex: /\barn:aws:s3:::[a-zA-Z0-9._-]+\b/g,
    matchAccuracy: 'high',
  },
  // Alibaba Cloud
  {
    name: 'alibabaAccessKeyId',
    description: 'Alibaba Cloud AccessKey ID',
    regex: /\bLTAI[a-zA-Z0-9]{20}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsSecretAccessKey',
    description: 'AWS secret access key',
    regex:
      /\b['"]?(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9/+=]{40}['"]?\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'awsSessionToken',
    description: 'AWS session token',
    regex:
      /\b['"]?(?:AWS|aws|Aws)?_?(?:SESSION|session|Session)_?(?:TOKEN|token|Token)['"]?\s*(?::|=>|=)\s*['"]?[A-Za-z0-9/+=]{200,}['"]?\b/g,
    matchAccuracy: 'high',
  },
  // Secrets Manager Secret ARN
  {
    name: 'awsSecretsManagerArn',
    description: 'AWS Secrets Manager secret ARN',
    regex:
      /\barn:aws:secretsmanager:[a-z0-9-]+:[0-9]{12}:secret:[a-zA-Z0-9/_+=.@-]+\b/g,
    matchAccuracy: 'high',
  },
];
