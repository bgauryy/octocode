export const CACHE_TTL_MS = 24 * 60 * 60 * 1000;

export const MAX_CONTENT_SIZE_BYTES = 1024 * 1024;
export const MAX_SKILL_FILES = 500;

export const LOCAL_SKILL_IGNORED_DIRS = new Set([
  '.git',
  '.hg',
  '.svn',
  'node_modules',
  'dist',
  'out',
  'target',
  'coverage',
  '.next',
  '.turbo',
]);

export const LOCAL_SKILL_IGNORED_FILES = new Set([
  '.DS_Store',
  'Thumbs.db',
  'npm-debug.log',
  'yarn-error.log',
]);

export const SKILLS_SH_API = 'https://www.skills.sh/api/search';
