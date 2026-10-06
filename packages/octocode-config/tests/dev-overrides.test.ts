import { describe, expect, it } from 'vitest';

import {
  configFieldEnvNames,
  contractDriftAllowed,
  contractDriftMessage,
  CONTRACT_DRIFT_OVERRIDE_ENV,
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

describe('contractDriftAllowed', () => {
  const on = { [CONTRACT_DRIFT_OVERRIDE_ENV]: '1' };

  it('requires the override env set to 1', () => {
    expect(contractDriftAllowed({}, { bundled: false })).toBe(false);
    expect(contractDriftAllowed({ [CONTRACT_DRIFT_OVERRIDE_ENV]: 'true' }, { bundled: false })).toBe(false);
    expect(contractDriftAllowed(on, { bundled: false })).toBe(true);
  });

  it('keeps production and unopted bundles fail-closed', () => {
    expect(contractDriftAllowed({ ...on, NODE_ENV: 'production' }, { bundled: false })).toBe(false);
    expect(contractDriftAllowed(on, { bundled: true })).toBe(false);
    expect(contractDriftAllowed({ ...on, NODE_ENV: 'development' }, { bundled: true })).toBe(true);
  });
});

describe('contractDriftMessage', () => {
  it('names both fingerprints and every override condition', () => {
    const message = contractDriftMessage('a'.repeat(64), 'b'.repeat(64));
    expect(message).toContain('Contract drift');
    expect(message).toContain('fingerprint mismatch');
    expect(message).toContain('a'.repeat(64));
    expect(message).toContain('b'.repeat(64));
    expect(message).toContain(`${CONTRACT_DRIFT_OVERRIDE_ENV}=1`);
    expect(message).toContain('NODE_ENV=development');
    expect(message).toContain('NODE_ENV=production always fails closed');
  });
});

describe('configFieldEnvNames', () => {
  it('lists a field env bindings in priority order', () => {
    expect(configFieldEnvNames('classification.api')).toEqual([
      'OCTOCODE_CLASSIFICATION_API',
    ]);
    expect(configFieldEnvNames('local.beta')).toEqual(['OCTOCODE_BETA']);
  });

  it('is empty for an unknown path', () => {
    expect(configFieldEnvNames('nope.missing')).toEqual([]);
  });
});
