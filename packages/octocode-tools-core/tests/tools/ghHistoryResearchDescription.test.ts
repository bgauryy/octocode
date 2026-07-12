import { beforeEach, describe, expect, it } from 'vitest';
import { completeMetadata } from '@octocodeai/octocode-core';
import {
  _resetDescriptionOverrideCache,
  getPatchedToolMetadata,
} from '../../src/tools/toolMetadata/descriptionOverrides.js';
import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';

describe('ghHistoryResearch description contract', () => {
  beforeEach(() => {
    _resetDescriptionOverrideCache();
  });

  it('core ships without type:"issues" mention', () => {
    const raw = completeMetadata.tools.ghHistoryResearch?.description ?? '';
    expect(raw).toMatch(/type:"prs" searches PRs/);
    expect(raw).not.toMatch(/type:"issues"/);
  });

  it('patch adds type:"issues" and issueNumber to the description', () => {
    const patched =
      getPatchedToolMetadata(completeMetadata).tools.ghHistoryResearch
        ?.description ?? '';
    expect(patched).toMatch(/type:"issues"/);
    expect(patched).toMatch(/issueNumber/);
    expect(patched).toMatch(/type:"releases"/);
    // original prs/commits content is preserved
    expect(patched).toMatch(/type:"prs"/);
    expect(patched).toMatch(/type:"commits"/);
  });

  it('DESCRIPTIONS proxy reflects the patch', () => {
    expect(DESCRIPTIONS.ghHistoryResearch).toMatch(/type:"issues"/);
    expect(DESCRIPTIONS.ghHistoryResearch).toMatch(/issueNumber/);
  });

  it('localFindFiles patch still applies in the same pass', () => {
    const patched = getPatchedToolMetadata(completeMetadata);
    expect(
      patched.tools?.localFindFiles?.description
    ).toMatch(/Nothing is excluded by default/);
  });
});
