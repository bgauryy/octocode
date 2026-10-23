import { describe, expect, it } from 'vitest';

import {
  configFieldEnvName,
  devOverridesAllowed,
} from '../src/index.js';

describe('devOverridesAllowed', () => {
  it('never honours overrides under NODE_ENV=production', () => {
    expect(devOverridesAllowed({ NODE_ENV: 'production' }, { bundled: false })).toBe(false);
    expect(devOverridesAllowed({ NODE_ENV: 'production' }, { bundled: true })).toBe(false);
  });

  it('honours overrides from source unless production', () => {
    expect(devOverridesAllowed({}, { bundled: false })).toBe(true);
  });

  it('treats a bundle as production unless NODE_ENV opts in', () => {
    expect(devOverridesAllowed({}, { bundled: true })).toBe(false);
    expect(devOverridesAllowed({ NODE_ENV: 'staging' }, { bundled: true })).toBe(false);
    expect(devOverridesAllowed({ NODE_ENV: 'development' }, { bundled: true })).toBe(true);
    expect(devOverridesAllowed({ NODE_ENV: 'test' }, { bundled: true })).toBe(true);
  });
});

describe('configFieldEnvName', () => {
  it('names the one variable a field binds', () => {
    expect(configFieldEnvName('classification.api')).toBe(
      'OCTOCODE_CLASSIFICATION_API'
    );
    expect(configFieldEnvName('local.beta')).toBe('OCTOCODE_BETA');
  });

  it('is undefined for an unknown path', () => {
    expect(configFieldEnvName('nope.missing')).toBeUndefined();
  });
});
