import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { sharedConstantsSchema } from '../scripts/shared-constants-schema.js';
import {
  CONFIG_SOURCE_ENV_KEYS,
  ENV_TOKEN_VARS,
  PROTECTED_KEY_NAMES,
} from '../src/config/sharedConstants.generated.js';

const sourcePath = fileURLToPath(
  new URL('../shared-constants.json', import.meta.url)
);

function loadSource() {
  return sharedConstantsSchema.parse(
    JSON.parse(readFileSync(sourcePath, 'utf8')) as unknown
  );
}

describe('shared config constants', () => {
  it('keeps generated TypeScript values equal to the Zod-validated source', () => {
    const source = loadSource();
    expect(ENV_TOKEN_VARS).toEqual(source.envTokenVars);
    expect(PROTECTED_KEY_NAMES).toEqual(source.protectedKeys);
    expect(CONFIG_SOURCE_ENV_KEYS).toEqual(source.configSourceEnvKeys);
  });

  it('requires token variables to be protected', () => {
    const source = loadSource();
    const tokenVar = source.envTokenVars[0];
    if (!tokenVar) throw new Error('expected at least one token variable');
    source.protectedKeys = source.protectedKeys.filter(key => key !== tokenVar);
    expect(sharedConstantsSchema.safeParse(source).success).toBe(false);
  });

  it('rejects duplicate keys and inverted validation bounds', () => {
    const source = loadSource();
    const sourceKey = source.configSourceEnvKeys[0];
    const maxTimeout = source.validationBounds['maxTimeout'];
    if (!sourceKey || maxTimeout === undefined) {
      throw new Error('expected source key and timeout bounds');
    }
    source.configSourceEnvKeys.push(sourceKey);
    source.validationBounds['minTimeout'] = maxTimeout + 1;

    const result = sharedConstantsSchema.safeParse(source);
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues.map(issue => issue.message)).toEqual(
        expect.arrayContaining([
          expect.stringContaining('contains duplicate value'),
          expect.stringContaining('must not exceed'),
        ])
      );
    }
  });
});
