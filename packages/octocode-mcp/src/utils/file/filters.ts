export const IGNORED_FOLDER_NAMES = [
  '.github',
  '.git',
  '.vscode',
  '.devcontainer',
  '.config',
  '.cargo',
  '.changeset',
  '.husky',
  '.aspect',
  '.eslint-plugin-local',
  '.yarn',
  '.gemini',
  '.ng-dev',
  '.configurations',
  '.tx',
  'dist',
  'build',
  'out',
  'output',
  'target',
  'release',
  'node_modules',
  'vendor',
  'third_party',
  'tmp',
  'temp',
  'cache',
  '.cache',
  '.tmp',
  '.pytest_cache',
  '.tox',
  '.venv',
  '.mypy_cache',
  '.next',
  '.svelte-kit',
  '.turbo',
  '.angular',
  '.dart_tool',
  '__pycache__',
  '.ruff_cache',
  '.nox',
  'htmlcov',
  'cover',
  '.gradle',
  '.m2',
  '.sbt',
  '.bloop',
  '.metals',
  '.bsp',
  'bin',
  'obj',
  'TestResults',
  'BenchmarkDotNet.Artifacts',
  '.vendor-new',
  'Godeps',
  'composer.phar',
  '.phpunit.result.cache',
  '.bundle',
  '.byebug_history',
  '.rspec_status',
  '.mvn',
  '.aws',
  '.gcp',
  'fastlane',
  'DerivedData',
  'xcuserdata',
  'local.properties',
  '.navigation',
  'captures',
  '.externalNativeBuild',
  '.cxx',
  '.idea',
  '.idea_modules',
  '.vs',
  '.history',
  'coverage',
  '.nyc_output',
  'logs',
  'log',
  '.DS_Store',
];

export const IGNORED_FILE_NAMES = [
  'package-lock.json',
  '.secrets',
  '.secret',
  'secrets.json',
  'secrets.yaml',
  'secrets.yml',
  'credentials.json',
  'credentials.yaml',
  'credentials.yml',
  'auth.json',
  'auth.yaml',
  'auth.yml',
  'api-keys.json',
  'api_keys.json',
  'service-account.json',
  'service_account.json',
  'private-key.pem',
  'private_key.pem',
  'id_rsa',
  'id_dsa',
  'id_ecdsa',
  'id_ed25519',
  'keyfile',
  'keyfile.json',
  'gcloud-service-key.json',
  'firebase-adminsdk.json',
  'google-services.json',
  'GoogleService-Info.plist',
  '.DS_Store',
  'Thumbs.db',
  'db.sqlite3',
  'db.sqlite3-journal',
  '.eslintcache',
  '.stylelintcache',
  '.node_repl_history',
  '.yarn-integrity',
  'celerybeat-schedule',
  'celerybeat.pid',
  'ThirdPartyNoticeText.txt',
  'ThirdPartyNotices.txt',
  'cglicenses.json',
  'cgmanifest.json',
];

export const IGNORED_FILE_EXTENSIONS = [
  '.lock',
  '.log',
  '.tmp',
  '.temp',
  '.cache',
  '.bak',
  '.backup',
  '.orig',
  '.swp',
  '.swo',
  '.rej',
  '.pid',
  '.seed',
  '.old',
  '.save',
  '.temporary',
  '.exe',
  '.dll',
  '.so',
  '.dylib',
  '.a',
  '.lib',
  '.o',
  '.obj',
  '.bin',
  '.class',
  '.pdb',
  '.dSYM',
  '.pyc',
  '.pyo',
  '.pyd',
  '.jar',
  '.war',
  '.ear',
  '.nar',
  '.db',
  '.sqlite',
  '.sqlite3',
  '.mdb',
  '.accdb',
  '.zip',
  '.tar',
  '.gz',
  '.bz2',
  '.xz',
  '.lz',
  '.lzma',
  '.Z',
  '.tgz',
  '.rar',
  '.7z',
  '.deb',
  '.rpm',
  '.pkg',
  '.dmg',
  '.msi',
  '.appx',
  '.snap',
  '.map',
  '.d.ts.map',
  '.min.js',
  '.min.css',
  '.key',
  '.pem',
  '.p12',
  '.pfx',
  '.crt',
  '.cer',
  '.der',
  '.csr',
  '.jks',
  '.keystore',
  '.truststore',
  '.kate-swp',
  '.gnome-desktop',
  '.sublime-project',
  '.sublime-workspace',
  '.iml',
  '.iws',
  '.ipr',
  '.patch',
  '.diff',
  '.prof',
  '.profile',
  '.trace',
  '.perf',
  '.coverage',
  '.egg-info',
  '.egg',
  '.mo',
  '.pot',
  '.setup',
  '.paket.template',
];

export function shouldIgnoreDir(folderName: string): boolean {
  return IGNORED_FOLDER_NAMES.includes(folderName);
}

export function shouldIgnoreFile(filePath: string): boolean {
  const fileName = filePath.split('/').pop() || '';

  for (const ext of IGNORED_FILE_EXTENSIONS) {
    if (fileName.endsWith(ext)) {
      return true;
    }
  }

  if (IGNORED_FILE_NAMES.includes(fileName)) {
    return true;
  }

  const pathParts = filePath.split('/');
  for (const part of pathParts) {
    if (IGNORED_FOLDER_NAMES.includes(part)) {
      return true;
    }
  }

  return false;
}

interface GetExtensionOptions {
  lowercase?: boolean;

  fallback?: string;
}

export function getExtension(
  filePath: string,
  options?: GetExtensionOptions
): string {
  const parts = filePath.split('.');

  if (parts.length <= 1 || (parts.length === 2 && parts[0] === '')) {
    return options?.fallback ?? '';
  }

  const ext = parts[parts.length - 1]!;
  return options?.lowercase ? ext.toLowerCase() : ext;
}
