import { describe, it, expect } from 'vitest';
import {
  IGNORED_FOLDER_NAMES,
  IGNORED_FILE_NAMES,
  IGNORED_FILE_EXTENSIONS,
  shouldIgnoreDir,
  shouldIgnoreFile,
  getExtension,
} from '../../src/utils/file/filters.js';

describe('fileFilters', () => {
  describe('IGNORED_FOLDER_NAMES', () => {
    it('should contain common hidden folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('.github');
      expect(IGNORED_FOLDER_NAMES).toContain('.git');
      expect(IGNORED_FOLDER_NAMES).toContain('.vscode');
      expect(IGNORED_FOLDER_NAMES).toContain('.config');
    });

    it('should contain build/distribution folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('dist');
      expect(IGNORED_FOLDER_NAMES).toContain('build');
      expect(IGNORED_FOLDER_NAMES).toContain('out');
      expect(IGNORED_FOLDER_NAMES).toContain('target');
    });

    it('should contain dependency folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('node_modules');
      expect(IGNORED_FOLDER_NAMES).toContain('vendor');
      expect(IGNORED_FOLDER_NAMES).toContain('third_party');
    });

    it('should contain cache directories', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('tmp');
      expect(IGNORED_FOLDER_NAMES).toContain('cache');
      expect(IGNORED_FOLDER_NAMES).toContain('.cache');
      expect(IGNORED_FOLDER_NAMES).toContain('.tmp');
    });

    it('should contain language-specific cache folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('__pycache__');
      expect(IGNORED_FOLDER_NAMES).toContain('.pytest_cache');
      expect(IGNORED_FOLDER_NAMES).toContain('.next');
      expect(IGNORED_FOLDER_NAMES).toContain('.gradle');
    });

    it('should contain IDE folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('.idea');
      expect(IGNORED_FOLDER_NAMES).toContain('.vs');
    });

    it('should contain coverage folders', () => {
      expect(IGNORED_FOLDER_NAMES).toContain('coverage');
      expect(IGNORED_FOLDER_NAMES).toContain('.nyc_output');
    });
  });

  describe('IGNORED_FILE_NAMES', () => {
    it('should contain lock files', () => {
      expect(IGNORED_FILE_NAMES).toContain('package-lock.json');
    });

    it('should contain sensitive/secret files', () => {
      expect(IGNORED_FILE_NAMES).toContain('.secrets');
      expect(IGNORED_FILE_NAMES).toContain('secrets.json');
      expect(IGNORED_FILE_NAMES).toContain('credentials.json');
      expect(IGNORED_FILE_NAMES).toContain('api-keys.json');
      expect(IGNORED_FILE_NAMES).toContain('service-account.json');
    });

    it('should contain private key files', () => {
      expect(IGNORED_FILE_NAMES).toContain('private-key.pem');
      expect(IGNORED_FILE_NAMES).toContain('id_rsa');
      expect(IGNORED_FILE_NAMES).toContain('id_dsa');
      expect(IGNORED_FILE_NAMES).toContain('id_ecdsa');
      expect(IGNORED_FILE_NAMES).toContain('id_ed25519');
    });

    it('should contain OS-specific files', () => {
      expect(IGNORED_FILE_NAMES).toContain('.DS_Store');
      expect(IGNORED_FILE_NAMES).toContain('Thumbs.db');
    });

    it('should contain database files', () => {
      expect(IGNORED_FILE_NAMES).toContain('db.sqlite3');
      expect(IGNORED_FILE_NAMES).toContain('db.sqlite3-journal');
    });

    it('should contain cache files', () => {
      expect(IGNORED_FILE_NAMES).toContain('.eslintcache');
      expect(IGNORED_FILE_NAMES).toContain('.stylelintcache');
    });
  });

  describe('IGNORED_FILE_EXTENSIONS', () => {
    it('should contain temporary file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.lock');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.log');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.tmp');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.cache');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.bak');
    });

    it('should contain compiled/binary file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.exe');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.dll');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.so');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.class');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.pyc');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.jar');
    });

    it('should contain database file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.db');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.sqlite');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.sqlite3');
    });

    it('should contain archive file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.zip');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.tar');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.gz');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.rar');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.7z');
    });

    it('should contain map and minified file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.map');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.d.ts.map');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.min.js');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.min.css');
    });

    it('should contain certificate/key file extensions', () => {
      expect(IGNORED_FILE_EXTENSIONS).toContain('.key');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.pem');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.p12');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.crt');
      expect(IGNORED_FILE_EXTENSIONS).toContain('.jks');
    });
  });

  describe('shouldIgnoreDir', () => {
    it('should return true for ignored directories', () => {
      expect(shouldIgnoreDir('node_modules')).toBe(true);
      expect(shouldIgnoreDir('.git')).toBe(true);
      expect(shouldIgnoreDir('dist')).toBe(true);
      expect(shouldIgnoreDir('build')).toBe(true);
      expect(shouldIgnoreDir('coverage')).toBe(true);
      expect(shouldIgnoreDir('.cache')).toBe(true);
    });

    it('should return false for non-ignored directories', () => {
      expect(shouldIgnoreDir('src')).toBe(false);
      expect(shouldIgnoreDir('lib')).toBe(false);
      expect(shouldIgnoreDir('test')).toBe(false);
      expect(shouldIgnoreDir('docs')).toBe(false);
      expect(shouldIgnoreDir('scripts')).toBe(false);
    });

    it('should be case-sensitive', () => {
      expect(shouldIgnoreDir('node_modules')).toBe(true);
      expect(shouldIgnoreDir('Node_Modules')).toBe(false);
      expect(shouldIgnoreDir('NODE_MODULES')).toBe(false);
    });

    it('should match exact names only', () => {
      expect(shouldIgnoreDir('node_modules')).toBe(true);
      expect(shouldIgnoreDir('node_modules_backup')).toBe(false);
      expect(shouldIgnoreDir('my_node_modules')).toBe(false);
    });
  });

  describe('shouldIgnoreFile', () => {
    describe('File Extension Filtering', () => {
      it('should ignore files with ignored extensions', () => {
        expect(shouldIgnoreFile('file.log')).toBe(true);
        expect(shouldIgnoreFile('archive.zip')).toBe(true);
        expect(shouldIgnoreFile('app.exe')).toBe(true);
        expect(shouldIgnoreFile('lib.so')).toBe(true);
        expect(shouldIgnoreFile('data.db')).toBe(true);
        expect(shouldIgnoreFile('bundle.min.js')).toBe(true);
        expect(shouldIgnoreFile('style.min.css')).toBe(true);
        expect(shouldIgnoreFile('private.key')).toBe(true);
        expect(shouldIgnoreFile('cert.pem')).toBe(true);
      });

      it('should not ignore files with allowed extensions', () => {
        expect(shouldIgnoreFile('index.ts')).toBe(false);
        expect(shouldIgnoreFile('index.js')).toBe(false);
        expect(shouldIgnoreFile('style.css')).toBe(false);
        expect(shouldIgnoreFile('README.md')).toBe(false);
        expect(shouldIgnoreFile('config.json')).toBe(false);
        expect(shouldIgnoreFile('package.json')).toBe(false);
      });

      it('should handle files without extensions', () => {
        expect(shouldIgnoreFile('Makefile')).toBe(false);
        expect(shouldIgnoreFile('Dockerfile')).toBe(false);
        expect(shouldIgnoreFile('LICENSE')).toBe(false);
      });

      it('should handle .d.ts.map correctly', () => {
        expect(shouldIgnoreFile('types.d.ts.map')).toBe(true);
        expect(shouldIgnoreFile('types.d.ts')).toBe(false);
      });
    });

    describe('File Name Filtering', () => {
      it('should ignore sensitive/secret files', () => {
        expect(shouldIgnoreFile('.secrets')).toBe(true);
        expect(shouldIgnoreFile('secrets.json')).toBe(true);
        expect(shouldIgnoreFile('credentials.json')).toBe(true);
        expect(shouldIgnoreFile('api-keys.json')).toBe(true);
        expect(shouldIgnoreFile('id_rsa')).toBe(true);
        expect(shouldIgnoreFile('private-key.pem')).toBe(true);
      });

      it('should ignore lock files', () => {
        expect(shouldIgnoreFile('package-lock.json')).toBe(true);
      });

      it('should ignore OS-specific files', () => {
        expect(shouldIgnoreFile('.DS_Store')).toBe(true);
        expect(shouldIgnoreFile('Thumbs.db')).toBe(true);
      });

      it('should ignore database files', () => {
        expect(shouldIgnoreFile('db.sqlite3')).toBe(true);
        expect(shouldIgnoreFile('db.sqlite3-journal')).toBe(true);
      });

      it('should not ignore non-blacklisted file names', () => {
        expect(shouldIgnoreFile('yarn.lock')).toBe(true);
        expect(shouldIgnoreFile('README.md')).toBe(false);
        expect(shouldIgnoreFile('index.ts')).toBe(false);
      });
    });

    describe('Path-Based Filtering', () => {
      it('should ignore files in ignored directories', () => {
        expect(shouldIgnoreFile('node_modules/package/index.js')).toBe(true);
        expect(shouldIgnoreFile('.git/config')).toBe(true);
        expect(shouldIgnoreFile('dist/bundle.js')).toBe(true);
        expect(shouldIgnoreFile('coverage/index.html')).toBe(true);
        expect(shouldIgnoreFile('.cache/data.json')).toBe(true);
      });

      it('should ignore files in nested ignored directories', () => {
        expect(shouldIgnoreFile('src/node_modules/package/index.js')).toBe(
          true
        );
        expect(shouldIgnoreFile('packages/pkg/.cache/data.json')).toBe(true);
        expect(shouldIgnoreFile('root/.git/hooks/pre-commit')).toBe(true);
      });

      it('should not ignore files in allowed directories', () => {
        expect(shouldIgnoreFile('src/index.ts')).toBe(false);
        expect(shouldIgnoreFile('lib/utils.js')).toBe(false);
        expect(shouldIgnoreFile('test/index.test.ts')).toBe(false);
        expect(shouldIgnoreFile('docs/README.md')).toBe(false);
      });

      it('should handle deeply nested paths', () => {
        expect(
          shouldIgnoreFile('src/components/Button/node_modules/package.json')
        ).toBe(true);
        expect(shouldIgnoreFile('packages/app/src/utils/helpers.ts')).toBe(
          false
        );
      });
    });

    describe('Edge Cases', () => {
      it('should handle empty string', () => {
        expect(shouldIgnoreFile('')).toBe(false);
      });

      it('should handle file names with multiple dots', () => {
        expect(shouldIgnoreFile('types.d.ts.map')).toBe(true);
        expect(shouldIgnoreFile('bundle.min.js')).toBe(true);
        expect(shouldIgnoreFile('config.test.ts')).toBe(false);
      });

      it('should handle paths with no slashes', () => {
        expect(shouldIgnoreFile('index.ts')).toBe(false);
        expect(shouldIgnoreFile('.DS_Store')).toBe(true);
        expect(shouldIgnoreFile('file.log')).toBe(true);
      });

      it('should handle paths ending with slash', () => {
        expect(shouldIgnoreFile('src/')).toBe(false);
        expect(shouldIgnoreFile('node_modules/')).toBe(true);
      });

      it('should be case-sensitive for extensions', () => {
        expect(shouldIgnoreFile('file.log')).toBe(true);
        expect(shouldIgnoreFile('file.LOG')).toBe(false);
        expect(shouldIgnoreFile('file.Log')).toBe(false);
      });

      it('should be case-sensitive for file names', () => {
        expect(shouldIgnoreFile('.DS_Store')).toBe(true);
        expect(shouldIgnoreFile('.ds_store')).toBe(false);
      });

      it('should be case-sensitive for directory names', () => {
        expect(shouldIgnoreFile('node_modules/package.json')).toBe(true);
        expect(shouldIgnoreFile('Node_Modules/package.json')).toBe(false);
      });
    });

    describe('Performance - Optimized Check Order', () => {
      it('should check extensions before file names (fast path)', () => {
        expect(shouldIgnoreFile('test.log')).toBe(true);
        expect(shouldIgnoreFile('data.zip')).toBe(true);
      });

      it('should check file names after extensions', () => {
        expect(shouldIgnoreFile('.DS_Store')).toBe(true);
        expect(shouldIgnoreFile('package-lock.json')).toBe(true);
      });

      it('should check paths last (most expensive)', () => {
        expect(shouldIgnoreFile('src/node_modules/index.js')).toBe(true);
        expect(shouldIgnoreFile('deep/nested/path/dist/bundle.js')).toBe(true);
      });
    });

    describe('Security-Critical Filtering', () => {
      it('should ignore SSH private keys', () => {
        expect(shouldIgnoreFile('id_rsa')).toBe(true);
        expect(shouldIgnoreFile('id_dsa')).toBe(true);
        expect(shouldIgnoreFile('id_ecdsa')).toBe(true);
        expect(shouldIgnoreFile('id_ed25519')).toBe(true);
      });

      it('should ignore certificate files', () => {
        expect(shouldIgnoreFile('cert.key')).toBe(true);
        expect(shouldIgnoreFile('private.pem')).toBe(true);
        expect(shouldIgnoreFile('cert.p12')).toBe(true);
        expect(shouldIgnoreFile('server.crt')).toBe(true);
      });

      it('should ignore credential files', () => {
        expect(shouldIgnoreFile('credentials.json')).toBe(true);
        expect(shouldIgnoreFile('service-account.json')).toBe(true);
        expect(shouldIgnoreFile('gcloud-service-key.json')).toBe(true);
      });

      it('should ignore secrets files', () => {
        expect(shouldIgnoreFile('.secrets')).toBe(true);
        expect(shouldIgnoreFile('secrets.json')).toBe(true);
        expect(shouldIgnoreFile('api-keys.json')).toBe(true);
      });
    });

    describe('Build and Distribution Artifacts', () => {
      it('should ignore build output', () => {
        expect(shouldIgnoreFile('dist/bundle.js')).toBe(true);
        expect(shouldIgnoreFile('build/index.html')).toBe(true);
        expect(shouldIgnoreFile('out/main.js')).toBe(true);
      });

      it('should ignore minified files', () => {
        expect(shouldIgnoreFile('app.min.js')).toBe(true);
        expect(shouldIgnoreFile('style.min.css')).toBe(true);
      });

      it('should ignore source maps', () => {
        expect(shouldIgnoreFile('app.js.map')).toBe(true);
        expect(shouldIgnoreFile('types.d.ts.map')).toBe(true);
      });
    });
  });

  describe('getExtension', () => {
    describe('Basic Extension Extraction', () => {
      it('should extract extension from simple file names', () => {
        expect(getExtension('file.txt')).toBe('txt');
        expect(getExtension('script.js')).toBe('js');
        expect(getExtension('styles.css')).toBe('css');
        expect(getExtension('data.json')).toBe('json');
      });

      it('should extract extension from paths with directories', () => {
        expect(getExtension('src/utils/file.ts')).toBe('ts');
        expect(getExtension('/absolute/path/to/file.py')).toBe('py');
        expect(getExtension('deeply/nested/dir/script.rb')).toBe('rb');
      });

      it('should handle files with multiple dots', () => {
        expect(getExtension('config.test.ts')).toBe('ts');
        expect(getExtension('bundle.min.js')).toBe('js');
        expect(getExtension('types.d.ts')).toBe('ts');
        expect(getExtension('data.backup.json')).toBe('json');
      });

      it('should return empty string for files without extension', () => {
        expect(getExtension('Makefile')).toBe('');
        expect(getExtension('Dockerfile')).toBe('');
        expect(getExtension('LICENSE')).toBe('');
        expect(getExtension('README')).toBe('');
      });

      it('should return empty string for dotfiles without extension', () => {
        expect(getExtension('.gitignore')).toBe('');
        expect(getExtension('.eslintrc')).toBe('');
        expect(getExtension('.env')).toBe('');
        expect(getExtension('.dockerignore')).toBe('');
      });

      it('should handle dotfiles with extension', () => {
        expect(getExtension('.eslintrc.json')).toBe('json');
        expect(getExtension('.prettierrc.yaml')).toBe('yaml');
        expect(getExtension('.env.local')).toBe('local');
      });
    });

    describe('Lowercase Option', () => {
      it('should preserve original case by default', () => {
        expect(getExtension('file.TXT')).toBe('TXT');
        expect(getExtension('file.JSON')).toBe('JSON');
        expect(getExtension('file.PyThOn')).toBe('PyThOn');
      });

      it('should convert to lowercase when option is set', () => {
        expect(getExtension('file.TXT', { lowercase: true })).toBe('txt');
        expect(getExtension('file.JSON', { lowercase: true })).toBe('json');
        expect(getExtension('file.PyThOn', { lowercase: true })).toBe('python');
      });

      it('should not affect already lowercase extensions', () => {
        expect(getExtension('file.txt', { lowercase: true })).toBe('txt');
        expect(getExtension('file.js', { lowercase: true })).toBe('js');
      });
    });

    describe('Fallback Option', () => {
      it('should return empty string by default when no extension', () => {
        expect(getExtension('Makefile')).toBe('');
        expect(getExtension('.gitignore')).toBe('');
      });

      it('should return fallback value when no extension and fallback is set', () => {
        expect(getExtension('Makefile', { fallback: 'txt' })).toBe('txt');
        expect(getExtension('.gitignore', { fallback: 'txt' })).toBe('txt');
        expect(getExtension('noext', { fallback: 'unknown' })).toBe('unknown');
      });

      it('should not use fallback when extension exists', () => {
        expect(getExtension('file.js', { fallback: 'txt' })).toBe('js');
        expect(getExtension('data.json', { fallback: 'txt' })).toBe('json');
      });
    });

    describe('Combined Options', () => {
      it('should apply both lowercase and fallback', () => {
        expect(
          getExtension('file.TXT', { lowercase: true, fallback: 'unknown' })
        ).toBe('txt');
        expect(
          getExtension('Makefile', { lowercase: true, fallback: 'txt' })
        ).toBe('txt');
        expect(
          getExtension('.gitignore', { lowercase: true, fallback: 'txt' })
        ).toBe('txt');
      });

      it('should match minifier behavior with lowercase: true, fallback: txt', () => {
        const minifierOptions = { lowercase: true, fallback: 'txt' };

        expect(getExtension('script.JS', minifierOptions)).toBe('js');
        expect(getExtension('styles.CSS', minifierOptions)).toBe('css');
        expect(getExtension('Makefile', minifierOptions)).toBe('txt');
        expect(getExtension('.gitignore', minifierOptions)).toBe('txt');
      });
    });

    describe('Edge Cases', () => {
      it('should handle empty string', () => {
        expect(getExtension('')).toBe('');
        expect(getExtension('', { fallback: 'txt' })).toBe('txt');
      });

      it('should handle paths ending with dot', () => {
        expect(getExtension('file.')).toBe('');
        expect(getExtension('file.', { fallback: 'txt' })).toBe('');
      });

      it('should handle multiple consecutive dots', () => {
        expect(getExtension('file..txt')).toBe('txt');
        expect(getExtension('file...js')).toBe('js');
      });

      it('should handle very long extensions', () => {
        expect(getExtension('file.typescript')).toBe('typescript');
        expect(getExtension('file.someVeryLongExtension')).toBe(
          'someVeryLongExtension'
        );
      });

      it('should handle single character extensions', () => {
        expect(getExtension('file.c')).toBe('c');
        expect(getExtension('file.h')).toBe('h');
        expect(getExtension('file.r')).toBe('r');
      });

      it('should handle numeric extensions', () => {
        expect(getExtension('file.123')).toBe('123');
        expect(getExtension('backup.2024')).toBe('2024');
      });
    });
  });
});
