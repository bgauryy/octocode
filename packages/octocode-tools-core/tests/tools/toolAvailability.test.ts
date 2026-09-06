import { expect, it } from 'vitest';
import { getToolAvailability } from '../../src/tools/toolAvailability.js';

const config = {
  local: { enabled: true, enableClone: true },
  storage: { mode: 'persistent' as const },
  tools: { enabled: null, disabled: null },
};

it.each(['localSearch', 'localGetFileContent', 'localAnalyzeGraph', 'lspGetSemantics', 'ghCloneRepo'])(
  'local gate takes precedence over allowlisting %s', name => {
    expect(getToolAvailability(name, { ...config, local: { ...config.local, enabled: false }, tools: { enabled: [name], disabled: null } }))
      .toEqual({ enabled: false, envVar: 'ENABLE_LOCAL' });
  }
);

it('requires persistent storage for clone and gives the effective gate', () => {
  expect(getToolAvailability('ghCloneRepo', { ...config, storage: { mode: 'memory' } }))
    .toEqual({ enabled: false, envVar: 'OCTOCODE_STORAGE_MODE' });
  expect(getToolAvailability('ghCloneRepo', { ...config, local: { enabled: true, enableClone: false } }))
    .toEqual({ enabled: false, envVar: 'ENABLE_CLONE' });
});

it('uses the allowlist before the denylist', () => {
  const tools = { enabled: ['npmSearch'], disabled: ['npmSearch'] };
  expect(getToolAvailability('npmSearch', { ...config, tools })).toEqual({ enabled: true });
  expect(getToolAvailability('ghSearch', { ...config, tools })).toEqual({ enabled: false, envVar: 'TOOLS_TO_RUN' });
  expect(getToolAvailability('npmSearch', { ...config, tools: { enabled: null, disabled: ['npmSearch'] } }))
    .toEqual({ enabled: false, envVar: 'DISABLE_TOOLS' });
});
