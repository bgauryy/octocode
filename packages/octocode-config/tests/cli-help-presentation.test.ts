import { describe, expect, it } from 'vitest';
import { toolHelpSection } from '../scripts/tool-contract/presentation.ts';
import { at, list, tool, tools } from './cli-presentation-fixture.ts';

describe('CLI catalog fields and help embedded by the native binary', () => {
  it('precomputes the catalog field list from the published row of each form', () => {
    for (const entry of tools)
      expect(String(entry.fields), String(entry.name)).toMatch(/^\S.*\]$/);
    expect(tool('ghStructure').fields).toBe(
      '[operation?:"tree", owner*, repo*, include?, materialize?, maxDepth?, path?, …] | [operation:"refs", owner*, repo*, ref?] | [operation:"languages", owner*, repo*]'
    );
    // One form: the published fields, required first, truncated.
    expect(tool('localSearch').fields).toMatch(
      /^\[matchString\*, path\*, .*…\]$/
    );
    // Several forms: each a flat row, the selector a field with its values
    // in that form, never a variant label wrapping the row.
    expect(tool('astSearch').fields).toContain(
      '[operation:"match", path*, rule*, '
    );
    expect(tool('structureSearch').fields).toBe(
      '[operation?:"tree", path*, entryType?, extensions?, include?, maxDepth?] | [operation:"files", path*, empty?, entryType?, extensions?, include?, maxDepth?, …]'
    );
    expect(String(tool('lspSearch').fields).split(' | ')).toEqual([
      '[operation?:"definition"|"typeDefinition"|"implementation", line*, path*, symbolName*, contextLines?, orderHint?, workspaceRoot?]',
      '[operation:"references", line*, path*, symbolName*, contextLines?, includeDeclaration?, orderHint?, …]',
      '[operation:"callers", line*, path*, symbolName*, orderHint?, workspaceRoot?]',
      '[operation:"callees"|"supertypes", line*, path*, symbolName*, orderHint?, workspaceRoot?]',
      '[operation:"subtypes", line*, path*, symbolName*, orderHint?, workspaceRoot?]',
      '[operation:"hover", line*, path*, symbolName*, orderHint?, workspaceRoot?]',
      '[operation:"documentSymbols"|"diagnostic", path*, workspaceRoot?]',
      '[operation:"workspaceSymbol", path*, symbolName*, workspaceRoot?]',
      '[operation:"workspaceSymbol", symbolName*, workspaceRoot*, path?]',
    ]);
    // No operation field: the field whose values differ is the selector.
    expect(tool('artifactSearch').fields).toMatch(
      /^\[ecosystem:"npm", packageName\*, version\?\] \| \[ecosystem:"npm", keywords\*\]/
    );
    for (const entry of tools)
      for (const label of Array.isArray(entry.variantNames) ? entry.variantNames : [])
        expect(String(entry.fields), String(entry.name)).not.toContain(
          `${String(label)}[`
        );
  });

  it('gives every tool a help section with its required fields', () => {
    for (const entry of tools) {
      const help = String(entry.help);
      expect(help, String(entry.name)).toMatch(/\nRequired/);
      expect(
        help.endsWith(`Fields: octocode schema ${String(entry.name)} (complete: --view full)`),
        String(entry.name)
      ).toBe(true);
    }
    expect(tool('localSearch').help).toMatch(/Required: .*path/);
    expect(tool('localSearch').help).toContain(
      "Examples:\n  octocode localSearch registerHandler src"
    );
  });
});

const flag = (name: string, field: string): string =>
  String(
    list(at(tool(name), 'cli').flags).find(entry => entry.field === field)!
      .description
  );

describe('CLI flags embedded by the native binary', () => {
  it('names each operation for a flag its forms describe differently', () => {
    const pageSize = flag('astSearch', 'pageSize');
    expect(pageSize).toMatch(/^match: Files per page/);
    expect(pageSize).toContain(' syntaxTree: Nodes per page');
    expect(pageSize).toContain(' symbols: Symbols per page');
    const operation = flag('ghStructure', 'operation');
    expect(operation).toMatch(/^tree: Paths grouped per directory/);
    expect(operation).toContain(' refs: Default branch and tags');
    // Forms that share a description share one label.
    expect(flag('ghGetHistoryItem', 'pageSize')).toMatch(
      /^pullRequest, commit, compare: .* issue: Rows of the requested section per page\.$/
    );
    // One description across forms stays unlabelled.
    expect(flag('lspSearch', 'pageSize')).toBe('Rows per page; default fills the response window.');
  });

  it('lets workspaceSymbol with --workspaceRoot read its one argument as the symbol', () => {
    const positionals = list(at(tool('lspSearch'), 'cli').positionals);
    expect(positionals[0]!.omitWhen).toEqual({
      operation: 'workspaceSymbol',
      field: 'workspaceRoot',
    });
  });
});

describe('toolHelpSection', () => {
  it('lists required fields per operation for a tool with operations', () => {
    const help = toolHelpSection('ghGetHistoryItem');
    expect(help).toContain('Required fields, by operation:');
    expect(help).toMatch(/pullRequest\s+.*number/);
    expect(help).toMatch(/compare\s+.*base, head/);
  });

  it('labels continuation forms with their executable operation', () => {
    const help = toolHelpSection('clasify');
    expect(help).not.toContain('sectionsResume');
    expect(help).toMatch(/sections\s+operation, assessment/);
  });
});
