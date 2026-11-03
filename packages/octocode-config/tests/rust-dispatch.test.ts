import { buildEnforcementContractIr } from '@octocodeai/octocode-core/schema';
import { describe, expect, it } from 'vitest';
import { queryBranches, renderDispatch, toolNames, type ChannelRegistry } from '../scripts/tool-contract/rust-dispatch.ts';

const registry: ChannelRegistry = {
  localSearch: {
    row: { nextPage: 'linear', unfilteredUnknown: 'branch' },
    collections: ['files'],
  },
  lspSearch: {
    row: { cycle: 'numbered', readDeclaration: { kind: 'branch', owner: ['localFetch'] } },
    collections: [],
  },
  ghCloneRepo: { row: {}, collections: [] },
};
const tools = Object.keys(registry);

describe('renderDispatch', () => {
  const rust = renderDispatch(tools, registry);

  it('wraps every tool query in one ToolQuery variant with a From conversion', () => {
    for (const [tool, type] of [
      ['LocalSearch', 'LocalSearchQuery'],
      ['LspSearch', 'LspSearchQuery'],
      ['GhCloneRepo', 'GhCloneRepoQuery'],
    ]) {
      expect(rust).toContain(`    ${tool}(${type}),`);
      expect(rust).toContain(`impl ::std::convert::From<${type}> for ToolQuery`);
    }
  });

  it('names each declared channel once, with its kind and owner', () => {
    expect(rust).toContain('pub enum LocalSearchChannel {\n    NextPage,\n    UnfilteredUnknown,\n}');
    expect(rust).toContain('Self::UnfilteredUnknown => "unfilteredUnknown"');
    expect(rust).toContain('Self::Cycle => ChannelKind::Numbered');
    expect(rust).toContain('Self::ReadDeclaration => &["localFetch"]');
    expect(rust).toContain('Self::NextPage => &["localSearch"]');
    expect(rust).toContain('pub enum LocalSearchCollection {\n    Files,\n}');
  });

  it('emits no channel enum for a tool without channels', () => {
    expect(rust).not.toContain('GhCloneRepoChannel');
    expect(rust).not.toContain('LspSearchCollection');
  });

  it('names the error channels shared by every tool', () => {
    const withErrors = renderDispatch(tools, registry, { retry: 'branch', restart: 'branch' });
    expect(withErrors).toContain('pub enum ErrorChannel {\n    Retry,\n    Restart,\n}');
    expect(withErrors).toContain('Self::Retry => &["localSearch", "lspSearch", "ghCloneRepo"]');
  });

  it('implements one Channel trait for every channel enum and lists every key', () => {
    const withErrors = renderDispatch(tools, registry, { retry: 'branch' });
    for (const name of ['LocalSearchChannel', 'LspSearchChannel', 'ErrorChannel'])
      expect(withErrors).toContain(`impl Channel for ${name} {`);
    expect(withErrors).toContain('pub trait Channel: Copy + \'static {');
    const specs = withErrors.slice(withErrors.indexOf('pub const ALL_CHANNELS'));
    expect(specs.match(/ChannelSpec \{ key:/g)).toHaveLength(2 + 2 + 1);
    expect(specs).toContain('key: LspSearchChannel::ReadDeclaration.key()');
    expect(specs).toContain('key: ErrorChannel::Retry.key()');
  });

  it('converts each named query branch into ToolQuery through its tool query', () => {
    const branches = queryBranches(
      `impl ::std::convert::From<LocalSearchQueryFiles> for LocalSearchQuery {
    fn from(value: LocalSearchQueryFiles) -> Self { Self::Files(value) }
}
impl ::std::convert::From<Other> for Unrelated { }`,
      tools
    );
    expect(branches).toEqual([{ tool: 'localSearch', type: 'LocalSearchQueryFiles' }]);
    const out = renderDispatch(tools, registry, {}, branches);
    expect(out).toContain(
      'impl ::std::convert::From<LocalSearchQueryFiles> for ToolQuery {\n    fn from(value: LocalSearchQueryFiles) -> Self {\n        Self::LocalSearch(LocalSearchQuery::from(value))'
    );
    expect(() =>
      renderDispatch(tools, registry, {}, [
        { tool: 'localSearch', type: 'Shared' },
        { tool: 'lspSearch', type: 'Shared' },
      ])
    ).toThrow(/shared by/);
  });

  it('refuses a registry that does not match the tools', () => {
    expect(() => renderDispatch([...tools, 'newTool'], registry)).toThrow(/no entry for newTool/);
    expect(() => renderDispatch(['localSearch'], registry)).toThrow(/unknown tools/);
  });
});

describe('the core contract', () => {
  const ir = buildEnforcementContractIr();
  const rust = renderDispatch(
    toolNames(ir.tools),
    ir.continuationChannels,
    ir.errorContinuationChannels
  );

  it('maps every continuation channel key to exactly one enum variant', () => {
    for (const [tool, { row }] of Object.entries(ir.continuationChannels)) {
      const keys = Object.keys(row);
      if (keys.length === 0) continue;
      for (const key of keys)
        expect(rust.split(`=> "${key}",`).length - 1, `${tool}.${key}`).toBeGreaterThanOrEqual(1);
      const name = `${tool.charAt(0).toUpperCase()}${tool.slice(1)}Channel`;
      const body = /pub enum \w+ \{([^}]*)\}/.exec(rust.slice(rust.indexOf(`pub enum ${name} {`)))?.[1] ?? '';
      expect(body.match(/\w+,/g)).toHaveLength(keys.length);
    }
  });
});
