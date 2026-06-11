import { describe, expect, it } from 'vitest';
import CleanCSS from 'clean-css';
import { load as loadYaml } from 'js-yaml';
import { minify as htmlMinifierTerser } from 'html-minifier-terser';
import {
  applyContentViewMinification,
  MINIFICATION_MODES,
  minifyContent,
  minifyContentSync,
  minifyWithTerserSync,
} from '@octocodeai/octocode-minifier';

function expectJavaScriptParses(content: string): void {
  const parsed = minifyWithTerserSync(content);
  expect(parsed.failed).toBe(false);
}

function expectCssParses(content: string): void {
  const result = new CleanCSS({ level: 0 }).minify(content);
  expect(result.errors).toEqual([]);
}

async function expectHtmlParses(content: string): Promise<void> {
  await expect(
    htmlMinifierTerser(content, { collapseWhitespace: true })
  ).resolves.toEqual(expect.any(String));
}

function parseFlatKeyValueLines(content: string): ReadonlyMap<string, string> {
  const values = new Map<string, string>();

  for (const rawLine of content.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('[') || line.startsWith('#')) continue;

    const separatorIndex = line.indexOf('=');
    if (separatorIndex === -1) continue;

    values.set(
      line.slice(0, separatorIndex).trim(),
      line.slice(separatorIndex + 1).trim()
    );
  }

  return values;
}

describe('parser-backed minification validity', () => {
  it('documents the three public minification modes', () => {
    expect(MINIFICATION_MODES.contentView.mode).toBe('content-view');
    expect(MINIFICATION_MODES.minify.mode).toBe('minify');
    expect(MINIFICATION_MODES.symbols.mode).toBe('symbols');
  });

  it('keeps content-view source-like while full TS minify emits valid JavaScript', async () => {
    const source = `
// agent-visible type context
type User = {
  id: string;
  name: string;
};

export const user: User = {
  id: '1',
  name: 'Ada Lovelace',
};
`;

    const contentView = applyContentViewMinification(source, 'user.ts');
    const minified = await minifyContent(source, 'user.ts');

    expect(contentView).toContain('type User');
    expect(minified.failed).toBe(false);
    expect(minified.content).not.toContain('type User');
    expect(minified.content).toContain('export const user');
    expect(minified.content.length).toBeLessThan(source.length);
    expectJavaScriptParses(minified.content);
  });

  it('uses parser-backed full minification for TSX output that still parses', async () => {
    const source = `
import React from 'react';

export type Props = {
  title: string;
  count: number;
};

export function Counter(props: Props) {
  return (
    <section className="counter">
      <h1>{props.title}</h1>
      <button type="button">{props.count}</button>
    </section>
  );
}
`;

    const minified = await minifyContent(source, 'Counter.tsx');

    expect(minified.failed).toBe(false);
    expect(minified.content).toContain('react/jsx-runtime');
    expect(minified.content).not.toContain('Props');
    expect(minified.content.length).toBeLessThan(source.length);
    expectJavaScriptParses(minified.content);
  });

  it('minifies embedded HTML style and script blocks with parser-backed engines', async () => {
    const source = `
<!doctype html>
<html>
  <head>
    <style>
      .box {
        color: red;
        padding: 4px;
      }
    </style>
  </head>
  <body>
    <script>
      function add(left, right) {
        return left + right;
      }
      add(1, 2);
    </script>
  </body>
</html>
`;

    const minified = await minifyContent(source, 'index.html');

    expect(minified.failed).toBe(false);
    expect(minified.content).toContain('.box{color:red;padding:4px}');
    expect(minified.content).toContain('function add(left,right)');
    expect(minified.content.length).toBeLessThan(source.length);
    expectCssParses('.box{color:red;padding:4px}');
    await expectHtmlParses(minified.content);
  });

  it('minifies Vue/Svelte-style embedded script and style blocks', async () => {
    const source = `
<template>
  <!-- redundant comment -->
  <section class="panel">{{ title }}</section>
</template>
<script>
  export function double(value) {
    return value * 2;
  }
</script>
<style>
  .panel {
    display: block;
    color: blue;
  }
</style>
`;

    const minified = await minifyContent(source, 'Panel.vue');

    expect(minified.failed).toBe(false);
    expect(minified.content).not.toContain('redundant comment');
    expect(minified.content).toContain('export function double(value)');
    expect(minified.content).toContain('.panel{display:block;color:#00f}');
    expect(minified.content.length).toBeLessThan(source.length);
  });

  it('keeps JSON and YAML parseable after minification', () => {
    const json = minifyContentSync(
      `{
        // remove
        "name": "octocode",
        "enabled": true,
      }`,
      'config.jsonc'
    );
    const yaml = minifyContentSync(
      `
name: octocode # remove
enabled: true
items:
  - search
  - minify
`,
      'config.yaml'
    );

    expect(JSON.parse(json)).toEqual({ name: 'octocode', enabled: true });
    expect(loadYaml(yaml)).toEqual({
      name: 'octocode',
      enabled: true,
      items: ['search', 'minify'],
    });
  });

  it('keeps TOML and INI line-structured enough for key-value validation', () => {
    const toml = minifyContentSync(
      `
title = "Octocode # not a comment"
# remove
[owner]
name = "Ada"
`,
      'config.toml'
    );
    const ini = minifyContentSync(
      `
; remove
[server]
host = localhost
port = 3000
`,
      'settings.ini'
    );

    const tomlValues = parseFlatKeyValueLines(toml);
    const iniValues = parseFlatKeyValueLines(ini);

    expect(toml).toContain('\n');
    expect(ini).toContain('\n');
    expect(tomlValues.get('title')).toBe('"Octocode # not a comment"');
    expect(tomlValues.get('name')).toBe('"Ada"');
    expect(iniValues.get('host')).toBe('localhost');
    expect(iniValues.get('port')).toBe('3000');
  });
});
