export const ALLOWED_COMMANDS = ['rg', 'ls', 'find', 'grep', 'git'] as const;

export const DANGEROUS_PATTERNS = [
  /[;&|`$(){}[\]<>]/, // Shell metacharacters
  /\${/,
  /\$\(/,
] as const;

export const PATTERN_DANGEROUS_PATTERNS = [
  /\${/,
  /\$\(/,
  /`/, // Backtick substitution
  /;/,
] as const;
