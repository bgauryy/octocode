export const ALLOWED_COMMANDS = [
  'rg',
  'ls',
  'find',
  'grep',
  'git',
  'file',
  'zcat',
  'gunzip',
  'bzcat',
  'xzcat',
  'zstdcat',
  'zstd',
  'lz4cat',
  'brotli',
  'lzfse',
  'tar',
  'unzip',
  'bsdtar',
  '7z',
  '7zz',
] as const;

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
