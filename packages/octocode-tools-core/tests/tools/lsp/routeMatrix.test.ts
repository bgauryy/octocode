import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';

const native = createRequire(import.meta.url)(
  '@octocodeai/octocode-engine'
) as {
  detectLanguageId(path: string): string | undefined;
  getLanguageServerForFile(
    path: string,
    root: string
  ): { languageId: string; command: string; args: string[] } | null;
};

// Explicit shipped routing contract. Native syntax grammars are a separate
// matrix: shell/Less/Elixir routing does not imply AST support, for example.
const routes = [
  ['typescript', ['ts', 'mts', 'cts']],
  ['typescriptreact', ['tsx']],
  ['javascript', ['js', 'mjs', 'cjs']],
  ['javascriptreact', ['jsx']],
  ['python', ['py', 'pyi']],
  ['shellscript', ['sh']],
  ['go', ['go']],
  ['rust', ['rs']],
  ['java', ['java']],
  ['c', ['c', 'h']],
  ['cpp', ['cpp', 'cc', 'cxx', 'hpp', 'hh', 'hxx']],
  ['csharp', ['cs']],
  ['json', ['json', 'jsonc']],
  ['yaml', ['yaml', 'yml']],
  ['html', ['html', 'htm']],
  ['css', ['css']],
  ['scss', ['scss']],
  ['less', ['less']],
  ['php', ['php']],
  ['sql', ['sql']],
  ['swift', ['swift']],
  ['ruby', ['rb', 'rake', 'gemspec', 'ru']],
  ['kotlin', ['kt', 'kts']],
  ['elixir', ['ex', 'exs']],
  ['scala', ['scala', 'sc']],
] as const;
const cases = routes.flatMap(([language, extensions]) =>
  extensions.map(extension => ({ language, extension }))
);

describe('shipped LSP route acceptance', () => {
  it('covers the documented 25 language IDs and 45 extensions', () => {
    expect(new Set(routes.map(([language]) => language)).size).toBe(25);
    expect(new Set(cases.map(({ extension }) => extension)).size).toBe(45);
  });
  it.each(cases)(
    '$extension selects $language independently of extension casing',
    ({ language, extension }) => {
      for (const suffix of [extension, extension.toUpperCase()]) {
        const file = `/tmp/octocode-route-acceptance/fixture.${suffix}`;
        expect(native.detectLanguageId(file)).toBe(language);
        const config = native.getLanguageServerForFile(
          file,
          '/tmp/octocode-route-acceptance'
        );
        expect(config?.languageId).toBe(language);
        expect(config?.command).toEqual(expect.any(String));
        expect(config?.command.length).toBeGreaterThan(0);
        expect(config?.args).toEqual(expect.any(Array));
      }
    }
  );
  it.each(['sbt', 'toml', 'lua', 'zig', 'md', 'mdx', 'unknown'])(
    'does not invent a built-in server route for .%s',
    extension => {
      expect(
        native.getLanguageServerForFile(
          `/tmp/octocode-route-acceptance/fixture.${extension}`,
          '/tmp/octocode-route-acceptance'
        )
      ).toBeNull();
    }
  );
});
