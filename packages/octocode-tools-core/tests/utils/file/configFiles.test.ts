import { describe, it, expect } from 'vitest';
import {
  isConfigFile,
  isLockFile,
  classifyFileType,
  CONFIG_FILENAMES,
  LOCK_FILENAMES,
} from '../../../src/utils/file/configFiles.js';

describe('isConfigFile', () => {
  it('matches exact manifest basenames', () => {
    expect(isConfigFile('package.json')).toBe(true);
    expect(isConfigFile('Cargo.toml')).toBe(true);
    expect(isConfigFile('go.mod')).toBe(true);
    expect(isConfigFile('pyproject.toml')).toBe(true);
    expect(isConfigFile('Gemfile')).toBe(true);
  });

  it('matches manifests inside a nested path', () => {
    expect(isConfigFile('/repo/packages/app/package.json')).toBe(true);
    expect(isConfigFile('backend\\service\\go.mod')).toBe(true);
  });

  it('matches newly added top-language manifests', () => {
    for (const name of [
      '.yarnrc.yml',
      'turbo.json',
      'go.work',
      'build.gradle.kts',
      'settings.gradle.kts',
      'global.json',
      'Directory.Build.props',
      '.node-version',
      '.python-version',
    ]) {
      expect(isConfigFile(name), name).toBe(true);
    }
  });

  it('does NOT treat lockfiles as config', () => {
    for (const name of [
      'yarn.lock',
      'package-lock.json',
      'pnpm-lock.yaml',
      'Cargo.lock',
      'go.sum',
      'poetry.lock',
      'composer.lock',
      'Gemfile.lock',
      'bun.lockb',
      'uv.lock',
      '.terraform.lock.hcl',
    ]) {
      expect(isConfigFile(name), name).toBe(false);
    }
  });

  it('matches variadic/templated config names', () => {
    expect(isConfigFile('.eslintrc.json')).toBe(true);
    expect(isConfigFile('tsconfig.build.json')).toBe(true);
    expect(isConfigFile('.env.production')).toBe(true);
    expect(isConfigFile('MyApp.csproj')).toBe(true);
    expect(isConfigFile('MyLib.fsproj')).toBe(true);
    expect(isConfigFile('main.tf')).toBe(true);
  });

  it('matches structured config extensions', () => {
    expect(isConfigFile('settings.ini')).toBe(true);
    expect(isConfigFile('app.conf')).toBe(true);
    expect(isConfigFile('gradle.properties')).toBe(true);
  });

  it('no longer treats localization files as config', () => {
    expect(isConfigFile('messages.po')).toBe(false);
    expect(isConfigFile('messages.mo')).toBe(false);
    expect(isConfigFile('template.pot')).toBe(false);
  });

  it('rejects non-config paths and empty input', () => {
    expect(isConfigFile('src/index.ts')).toBe(false);
    expect(isConfigFile('README.md')).toBe(false);
    expect(isConfigFile('')).toBe(false);
  });

  it('exposes a de-duplicated filename list', () => {
    expect(new Set(CONFIG_FILENAMES).size).toBe(CONFIG_FILENAMES.length);
  });
});

describe('isLockFile', () => {
  it('matches lockfiles across ecosystems', () => {
    for (const name of [
      'yarn.lock',
      'package-lock.json',
      'npm-shrinkwrap.json',
      'pnpm-lock.yaml',
      'bun.lockb',
      'Cargo.lock',
      'go.sum',
      'poetry.lock',
      'uv.lock',
      'Pipfile.lock',
      'composer.lock',
      'Gemfile.lock',
      'pubspec.lock',
      '.terraform.lock.hcl',
      // linguist generated.rb additions
      'deno.lock',
      'pdm.lock',
      'pixi.lock',
      'Gopkg.lock',
      'glide.lock',
      'Package.resolved',
      'flake.lock',
      'MODULE.bazel.lock',
      'mise.lock',
      'Cargo.toml.orig',
    ]) {
      expect(isLockFile(name), name).toBe(true);
    }
  });

  it('matches lockfiles inside a nested path', () => {
    expect(isLockFile('/repo/frontend/yarn.lock')).toBe(true);
  });

  it('rejects config/manifest and empty input', () => {
    expect(isLockFile('package.json')).toBe(false);
    expect(isLockFile('Cargo.toml')).toBe(false);
    expect(isLockFile('')).toBe(false);
  });

  it('exposes a de-duplicated lock list disjoint from config', () => {
    expect(new Set(LOCK_FILENAMES).size).toBe(LOCK_FILENAMES.length);
    const configSet = new Set(CONFIG_FILENAMES);
    for (const name of LOCK_FILENAMES) {
      expect(configSet.has(name), name).toBe(false);
    }
  });
});

describe('classifyFileType', () => {
  it('classifies config manifests, with config winning over code/data ext', () => {
    expect(classifyFileType('package.json')).toBe('config');
    expect(classifyFileType('vite.config.js')).toBe('config');
    expect(classifyFileType('pyproject.toml')).toBe('config');
    expect(classifyFileType('Dockerfile')).toBe('config');
  });

  it('classifies lockfiles as lock, not config', () => {
    expect(classifyFileType('yarn.lock')).toBe('lock');
    expect(classifyFileType('package-lock.json')).toBe('lock');
    expect(classifyFileType('Cargo.lock')).toBe('lock');
    expect(classifyFileType('go.sum')).toBe('lock');
    expect(classifyFileType('pnpm-lock.yaml')).toBe('lock');
  });

  it('classifies documentation files', () => {
    expect(classifyFileType('README.md')).toBe('doc');
    expect(classifyFileType('docs/guide.mdx')).toBe('doc');
    expect(classifyFileType('LICENSE')).toBe('doc');
    expect(classifyFileType('LICENSE-MIT')).toBe('doc');
    expect(classifyFileType('CHANGELOG')).toBe('doc');
    expect(classifyFileType('notes.rst')).toBe('doc');
    expect(classifyFileType('INSTALL')).toBe('doc');
    expect(classifyFileType('CITATION.cff')).toBe('doc');
    expect(classifyFileType('page.textile')).toBe('doc');
  });

  it('classifies source code for top languages', () => {
    expect(classifyFileType('src/index.ts')).toBe('code');
    expect(classifyFileType('main.py')).toBe('code');
    expect(classifyFileType('lib.rs')).toBe('code');
    expect(classifyFileType('server.go')).toBe('code');
    expect(classifyFileType('App.java')).toBe('code');
    expect(classifyFileType('script.sh')).toBe('code');
  });

  it('does not let a doc-basename word shadow a real code/config extension', () => {
    expect(classifyFileType('install.sh')).toBe('code');
    expect(classifyFileType('install.py')).toBe('code');
    expect(classifyFileType('history.py')).toBe('code');
    expect(classifyFileType('notice.go')).toBe('code');
    expect(classifyFileType('authors.rb')).toBe('code');
    expect(classifyFileType('contributors.ts')).toBe('code');
    expect(classifyFileType('changes.ts')).toBe('code');
    expect(classifyFileType('citation.py')).toBe('code');
    expect(classifyFileType('license.go')).toBe('code');
    expect(classifyFileType('copying.c')).toBe('code');
    // still doc when genuinely extensionless or bare-suffixed
    expect(classifyFileType('install')).toBe('doc');
    expect(classifyFileType('LICENSE-MIT')).toBe('doc');
  });

  it('returns undefined (omit field) for unknown or binary-ish paths', () => {
    expect(classifyFileType('logo.png')).toBeUndefined();
    expect(classifyFileType('data.bin')).toBeUndefined();
    expect(classifyFileType('noextension')).toBeUndefined();
    expect(classifyFileType('')).toBeUndefined();
    expect(classifyFileType('dir/')).toBeUndefined();
  });
});
