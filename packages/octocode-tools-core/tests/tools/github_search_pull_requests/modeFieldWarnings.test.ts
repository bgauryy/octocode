import { describe, expect, it } from 'vitest';
import { modeFieldWarnings } from '../../../src/tools/github_search_pull_requests/execution.js';

// Misplaced-filter warnings must be symmetric: a commit-history filter passed
// to a mode that ignores it (prs/issues/releases) must say so in-band, the
// same way PR-search filters are already flagged in commits/releases.
describe('modeFieldWarnings — symmetric across all four modes', () => {
  it('warns about a commits-only field (since) passed to prs mode', () => {
    const warnings = modeFieldWarnings({ since: '2024-01-01' }, 'prs');
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain('prs mode');
    expect(warnings[0]).toContain('since');
  });

  it('warns about a commits-only field (branch) passed to issues mode', () => {
    const warnings = modeFieldWarnings({ branch: 'main' }, 'issues');
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain('issues mode');
    expect(warnings[0]).toContain('branch');
  });

  it('still warns about PR-search filters in commits mode', () => {
    const warnings = modeFieldWarnings(
      { label: 'bug', state: 'open' },
      'commits'
    );
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain('commits mode');
    expect(warnings[0]).toContain('label');
    expect(warnings[0]).toContain('state');
  });

  it('releases mode flags both PR-search and commit-history filters', () => {
    const warnings = modeFieldWarnings(
      { label: 'bug', includeDiff: true },
      'releases'
    );
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain('label');
    expect(warnings[0]).toContain('includeDiff');
  });

  it('does not warn when a field is native to the mode (since in commits)', () => {
    expect(modeFieldWarnings({ since: '2024-01-01' }, 'commits')).toEqual([]);
  });

  it('does not warn about perPage in prs mode (has a default, not commits-exclusive)', () => {
    expect(modeFieldWarnings({ perPage: 30 }, 'prs')).toEqual([]);
  });
});
