import { describe, it, expect } from 'vitest';
import {
  IGNORED_FOLDER_NAMES,
  IGNORED_FILE_NAMES,
  IGNORED_FILE_EXTENSIONS,
  shouldIgnoreDir,
  shouldIgnoreFile,
} from '../../../octocode-tools-core/src/utils/file/filters.js';

describe('GitHub View Repo Structure Filters', () => {
  describe('IGNORED_FOLDER_NAMES', () => {
    it('should have complete list of ignored folders', () => {
      expect(IGNORED_FOLDER_NAMES).toEqual([
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
        '.DS_Store',
      ]);
    });
  });

  describe('IGNORED_FILE_NAMES', () => {
    it('should have complete list of ignored files', () => {
      expect(IGNORED_FILE_NAMES).toEqual([
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
      ]);
    });
  });

  describe('IGNORED_FILE_EXTENSIONS', () => {
    it('should have complete list of ignored extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toEqual([
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
      ]);
    });
  });

  describe('shouldIgnoreDir', () => {
    it('should ignore exact matches', () => {
      expect(shouldIgnoreDir('.github')).toEqual(true);
    });

    it('should ignore node_modules', () => {
      expect(shouldIgnoreDir('node_modules')).toEqual(true);
    });

    it('should ignore dist', () => {
      expect(shouldIgnoreDir('dist')).toEqual(true);
    });

    it('should ignore build', () => {
      expect(shouldIgnoreDir('build')).toEqual(true);
    });

    it('should not ignore src', () => {
      expect(shouldIgnoreDir('src')).toEqual(false);
    });

    it('should not ignore components', () => {
      expect(shouldIgnoreDir('components')).toEqual(false);
    });

    it('should not ignore utils', () => {
      expect(shouldIgnoreDir('utils')).toEqual(false);
    });
  });

  describe('shouldIgnoreFileByPath', () => {
    it('should ignore package-lock.json', () => {
      expect(shouldIgnoreFile('package-lock.json')).toEqual(true);
    });

    it('should ignore .DS_Store', () => {
      expect(shouldIgnoreFile('.DS_Store')).toEqual(true);
    });

    it('should ignore Thumbs.db', () => {
      expect(shouldIgnoreFile('Thumbs.db')).toEqual(true);
    });

    it('should not ignore .gitignore', () => {
      expect(shouldIgnoreFile('.gitignore')).toEqual(false);
    });

    it('should not ignore tsconfig.json', () => {
      expect(shouldIgnoreFile('tsconfig.json')).toEqual(false);
    });

    it('should not ignore LICENSE', () => {
      expect(shouldIgnoreFile('LICENSE')).toEqual(false);
    });

    it('should ignore temp.tmp', () => {
      expect(shouldIgnoreFile('temp.tmp')).toEqual(true);
    });

    it('should ignore cache.cache', () => {
      expect(shouldIgnoreFile('cache.cache')).toEqual(true);
    });

    it('should ignore backup.bak', () => {
      expect(shouldIgnoreFile('backup.bak')).toEqual(true);
    });

    it('should ignore app.min.js', () => {
      expect(shouldIgnoreFile('app.min.js')).toEqual(true);
    });

    it('should ignore styles.min.css', () => {
      expect(shouldIgnoreFile('styles.min.css')).toEqual(true);
    });

    it('should ignore bundle.map', () => {
      expect(shouldIgnoreFile('bundle.map')).toEqual(true);
    });

    it('should ignore .yarn/x/y/z.js', () => {
      expect(shouldIgnoreFile('.yarn/x/y/z.js')).toEqual(true);
    });

    it('should ignore node_modules/package/index.js', () => {
      expect(shouldIgnoreFile('node_modules/package/index.js')).toEqual(true);
    });

    it('should ignore dist/bundle.js', () => {
      expect(shouldIgnoreFile('dist/bundle.js')).toEqual(true);
    });

    it('should ignore .git/config', () => {
      expect(shouldIgnoreFile('.git/config')).toEqual(true);
    });

    it('should not ignore index.js', () => {
      expect(shouldIgnoreFile('index.js')).toEqual(false);
    });

    it('should not ignore App.tsx', () => {
      expect(shouldIgnoreFile('App.tsx')).toEqual(false);
    });

    it('should not ignore styles.css', () => {
      expect(shouldIgnoreFile('styles.css')).toEqual(false);
    });

    it('should not ignore README.md', () => {
      expect(shouldIgnoreFile('README.md')).toEqual(false);
    });

    it('should not ignore src/components/Button.tsx', () => {
      expect(shouldIgnoreFile('src/components/Button.tsx')).toEqual(false);
    });
  });
});
