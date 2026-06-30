// Discovery pruning lists: skip these dirs/files/extensions during tree walks.
// SYNC NOTE: pathPatterns.ts:IGNORED_PATH_PATTERNS and
// filePatterns.ts:IGNORED_FILE_PATTERNS overlap these lists (e.g. .git, .aws,
// .ssh, .docker, .kube, id_rsa, .pem, .key). The two systems have different
// roles — pathPatterns/filePatterns block *access* (read-time check);
// discoveryFilter blocks *visibility* (tree-walk pruning). Both must be kept
// in sync so that adding a sensitive path/file to one is reflected in the other.
export const DISCOVERY_IGNORED_FOLDER_NAMES = [
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
  '.ssh',
  '.kube',
  '.docker',
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
  '.DS_Store',
];

export const DISCOVERY_IGNORED_FILE_NAMES = [
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

export const DISCOVERY_IGNORED_FILE_EXTENSIONS = [
  '.lock',
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

export interface DiscoveryExtensionOptions {
  lowercase?: boolean;
  fallback?: string;
  leadingDot?: boolean;
}

export function shouldIgnoreDiscoveryDir(folderName: string): boolean {
  const normalized = folderName.replace(/\\/g, '/');
  const name = normalized.split('/').filter(Boolean).pop() ?? normalized;
  return DISCOVERY_IGNORED_FOLDER_NAMES.includes(name);
}

export function shouldIgnoreDiscoveryFile(filePath: string): boolean {
  const normalizedPath = filePath.replace(/\\/g, '/');
  const fileName = normalizedPath.split('/').pop() || '';

  for (const ext of DISCOVERY_IGNORED_FILE_EXTENSIONS) {
    if (fileName.endsWith(ext)) {
      return true;
    }
  }

  if (DISCOVERY_IGNORED_FILE_NAMES.includes(fileName)) {
    return true;
  }

  const pathParts = normalizedPath.split('/');
  for (const part of pathParts) {
    if (DISCOVERY_IGNORED_FOLDER_NAMES.includes(part)) {
      return true;
    }
  }

  return false;
}

export function getDiscoveryExtension(
  filePath: string,
  options?: DiscoveryExtensionOptions
): string {
  const basename = filePath.split(/[\\/]/).pop() ?? filePath;
  const fallback = options?.fallback ?? '';
  let ext = fallback;

  if (basename.startsWith('.')) {
    const dotfileExt = basename.slice(1);
    ext = dotfileExt.includes('.')
      ? (basename.split('.').pop() ?? fallback)
      : dotfileExt;
  } else {
    const lastDot = basename.lastIndexOf('.');
    ext = lastDot === -1 ? fallback : basename.slice(lastDot + 1);
  }

  const normalized = options?.lowercase ? ext.toLowerCase() : ext;
  if (!normalized || !options?.leadingDot) return normalized;
  return normalized.startsWith('.') ? normalized : `.${normalized}`;
}
