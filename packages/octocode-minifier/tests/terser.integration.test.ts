import { describe, it, expect } from 'vitest';
import { minifyContent, minifyWithTerser } from '@octocodeai/octocode-minifier';

describe('Terser integration — real dependency', () => {
  const javascript = `/*! license comment should be removed */
function longFunctionName(longArgumentName) {
  console.log(longArgumentName);
  debugger;
  return longArgumentName + 1;
}

longFunctionName(1);
`;

  it('uses options that strip comments without mangling names or dropping diagnostics', async () => {
    const result = await minifyWithTerser(javascript);

    expect(result.failed).toBe(false);
    expect(result.content).not.toContain('license comment');
    expect(result.content).toContain('longFunctionName');
    expect(result.content).toContain('longArgumentName');
    expect(result.content).toContain('console.log');
    expect(result.content).toContain('debugger');
    expect(result.content).toMatch(/;$/);
  });

  it('routes JavaScript files through the real Terser path', async () => {
    const result = await minifyContent(javascript, 'diagnostics.js');

    expect(result.type).toBe('terser');
    expect(result.failed).toBe(false);
    expect(result.content).not.toContain('license comment');
    expect(result.content).toContain('console.log');
    expect(result.content).toContain('debugger');
  });
});
