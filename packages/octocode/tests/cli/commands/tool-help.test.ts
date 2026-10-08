import { describe, expect, it } from 'vitest';
import {
  getPublicToolCatalogWithAddons,
  schemaUsageForms,
  type SchemaJsonObject,
} from '@octocodeai/config/schema';
import { toolHelpSection } from '../../../src/cli/commands/tool-help.js';

const tools = getPublicToolCatalogWithAddons()
  .tools as readonly SchemaJsonObject[];

function section(name: string): string {
  const tool = tools.find(candidate => candidate.name === name)!;
  return toolHelpSection(tool, schemaUsageForms(tool.querySchema));
}

describe('tool help section', () => {
  it.each(tools.map(tool => String(tool.name)))(
    '%s names its required fields, a runnable example, and the schema',
    name => {
      const text = section(name);
      expect(text).toMatch(/^Required/m);
      expect(text).toContain(`  octocode ${name} '{`);
      expect(text).toContain(
        `All fields: octocode schema ${name} --view query`
      );
      expect(text).not.toContain('/ABS/repo/');
      const example = /^ {2}octocode \S+ '([\s\S]*?)'$/m.exec(text)![1]!;
      expect(() => JSON.parse(example.replaceAll(`'\\''`, "'"))).not.toThrow();
      for (const line of text.split('\n'))
        expect(line.length, line).toBeLessThanOrEqual(130);
    }
  );

  it('lists one required form per branch and every variant', () => {
    expect(section('localFetch')).toContain('Required: path');
    expect(section('artifactSearch')).toContain('  ecosystem, packageName');
    const history = section('ghSearchHistory');
    for (const variant of ['pullRequest', 'issue', 'commit'])
      expect(history).toMatch(new RegExp(`^ {2}${variant} `, 'm'));
  });
});
