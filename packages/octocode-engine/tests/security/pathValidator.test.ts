import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

import { PathValidator } from '../../src/security/pathValidator.js';

describe('PathValidator', () => {
  let tempRoot: string | undefined;

  afterEach(() => {
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = undefined;
    }
  });

  it('allows paths through a realpath-equivalent allowed root', () => {
    tempRoot = mkdtempSync(join(tmpdir(), 'octocode-path-validator-'));
    const realRoot = realpathSync(tempRoot);
    const validator = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [realRoot],
    });

    const result = validator.validate(tempRoot);

    expect(result.isValid).toBe(true);
    expect(result.sanitizedPath).toBe(realRoot);
  });
});
