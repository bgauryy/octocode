import os from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  DISCOVERY_IGNORED_FOLDER_NAMES,
  getDiscoveryExtension,
  shouldIgnoreDiscoveryDir,
  shouldIgnoreDiscoveryFile,
} from '../../src/security/discoveryFilter.js';
import { shouldIgnore } from '../../src/security/ignoredPathFilter.js';

describe('discoveryFilter', () => {
  it('filters discovery-noise directories without using security path blocking', () => {
    expect(shouldIgnoreDiscoveryDir('node_modules')).toBe(true);
    expect(shouldIgnoreDiscoveryDir('packages/node_modules')).toBe(true);
    expect(shouldIgnoreDiscoveryDir('src')).toBe(false);
  });

  it('filters discovery-noise file names, extensions, and path segments', () => {
    expect(shouldIgnoreDiscoveryFile('src/app.ts')).toBe(false);
    expect(shouldIgnoreDiscoveryFile('package-lock.json')).toBe(true);
    expect(shouldIgnoreDiscoveryFile('dist/app.js')).toBe(true);
    expect(shouldIgnoreDiscoveryFile('bundle.min.js')).toBe(true);
    expect(shouldIgnoreDiscoveryFile('secrets.json')).toBe(true);
  });

  // SYNC CONTRACT: security-sensitive folders that discovery hides must ALSO be
  // blocked by the access-time ignoredPathFilter. Noise-only folders like
  // node_modules / dist are discovery-pruned but NOT access-blocked (tools
  // legitimately read from them). This test locks the security-sensitive subset
  // so that adding a sensitive path to one layer requires adding it to the other.
  it('security-sensitive discovery folders are also blocked by ignoredPathFilter', () => {
    const sensitiveNames = ['.aws', '.ssh', '.kube', '.docker'];
    const home = os.homedir();
    for (const name of sensitiveNames) {
      const fullPath = path.join(home, name);
      expect(
        shouldIgnore(fullPath),
        `ignoredPathFilter should block '${name}' under home dir`
      ).toBe(true);
      expect(
        shouldIgnoreDiscoveryDir(name),
        `discoveryFilter should hide '${name}'`
      ).toBe(true);
    }
  });

  it('security-sensitive folders are present in DISCOVERY_IGNORED_FOLDER_NAMES', () => {
    // node_modules is intentionally excluded: access-blocking it would break
    // legitimate tool reads; it belongs only in the discovery-pruning list.
    const securityCritical = ['.aws', '.ssh', '.kube', '.docker', '.git'];
    for (const name of securityCritical) {
      expect(
        DISCOVERY_IGNORED_FOLDER_NAMES,
        `'${name}' must be in DISCOVERY_IGNORED_FOLDER_NAMES`
      ).toContain(name);
    }
  });

  it('extracts extensions with dotfile and leading-dot options', () => {
    expect(getDiscoveryExtension('Foo.TS', { lowercase: true })).toBe('ts');
    expect(getDiscoveryExtension('.gitignore', { lowercase: true })).toBe(
      'gitignore'
    );
    expect(
      getDiscoveryExtension('assets/archive.tar.gz', {
        lowercase: true,
        leadingDot: true,
      })
    ).toBe('.gz');
    expect(getDiscoveryExtension('Makefile', { fallback: 'txt' })).toBe('txt');
  });
});
