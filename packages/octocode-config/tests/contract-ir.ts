// Core's contract IR for tests, built once per core build by contract-ir.global.ts.
import { readFileSync } from 'node:fs';
import type { buildEnforcementContractIr } from '@octocodeai/octocode-core/schema';
import { inject } from 'vitest';

export const contractIr = (): ReturnType<typeof buildEnforcementContractIr> =>
  JSON.parse(readFileSync(inject('contractIrPath'), 'utf8'));
