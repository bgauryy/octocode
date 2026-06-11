import { describe, it, expect } from 'vitest';
import {
  minifyContent,
  minifyContentSync,
  minifyWithTerser,
  minifyWithTerserSync,
} from '@octocodeai/octocode-minifier';

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

  it('uses real Terser for the sync JavaScript path', () => {
    const result = minifyWithTerserSync(javascript);

    expect(result.failed).toBe(false);
    expect(result.content).not.toContain('license comment');
    expect(result.content).toContain('longFunctionName');
    expect(result.content).toContain('longArgumentName');
    expect(result.content).toContain('console.log');
    expect(result.content).toContain('debugger');
    expect(result.content).toMatch(/;$/);
  });

  it('sync JavaScript minification preserves strings and regex literals', () => {
    const content = `
const url = "https://example.com/a//b";
const marker = "/* not a comment */";
const pattern = /https?:\\/\\/example\\.com\\/[/*]+/g;
// remove this comment
function check(value) {
  return pattern.test(value) && url.includes("//") && marker.includes("comment");
}
`;

    const result = minifyContentSync(content, 'strings-and-regex.js');

    expect(result).not.toContain('remove this comment');
    expect(result).toContain('https://example.com/a//b');
    expect(result).toContain('/* not a comment */');
    expect(result).toContain('/https?:\\/\\/example\\.com\\/[/*]+/g');
    expect(result).toContain('function check');
  });

  it('falls back to readable sync minification for JSX syntax Terser cannot parse', () => {
    const result = minifyWithTerserSync(`
const App = () => {
  return <div data-url="https://example.com/a//b">Hello</div>;
};
`);

    expect(result.failed).toBe(true);
    expect(result.reason).toContain('Terser sync minification failed');
    expect(result.content).toContain(
      '<div data-url="https://example.com/a//b">Hello</div>'
    );
    expect(result.content).toContain('const App');
  });

  it('keeps readable async JSX fallback when parser output would grow', async () => {
    const content = `
const App = () => {
  return <div data-url="https://example.com/a//b">Hello</div>;
};
`;

    const result = await minifyContent(content, 'App.jsx');

    expect(result.type).toBe('terser');
    expect(result.failed).toBe(false);
    expect(result.reason).toBeUndefined();
    expect(result.content).toContain(
      '<div data-url="https://example.com/a//b">Hello</div>'
    );
    expect(result.content.length).toBeLessThan(content.length);
  });

  it('returns readable async fallback content for Flow-annotated JavaScript', async () => {
    const content = `
// @flow
import type {Node} from 'react';
type Props = { children: Node };
export function Panel(props: Props): Node {
  return props.children;
}
`;

    const result = await minifyContent(content, 'Panel.js');

    expect(result.type).toBe('terser');
    expect(result.failed).toBe(false);
    expect(result.reason).toContain('Terser minification failed');
    expect(result.content).toContain('import type');
    expect(result.content).toContain("from 'react'");
    expect(result.content).toContain('type Props');
    expect(result.content.length).toBeLessThan(content.length);
  });
});
