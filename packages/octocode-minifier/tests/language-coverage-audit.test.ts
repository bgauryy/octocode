import { describe, expect, it } from 'vitest';
import {
  INDENTATION_SENSITIVE_NAMES,
  MINIFY_CONFIG,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from '@octocodeai/octocode-minifier';
import { buildLanguageBenchmarkCases } from './languageBenchmarkFixtures.js';
import { COMMON_REAL_LANGUAGE_EXTENSIONS } from './realLanguageBenchmark.js';

type PopularLanguageExpectation = {
  readonly language: string;
  readonly minifyExtensions: readonly string[];
  readonly symbolExtensions: readonly string[];
};

const POPULAR_LANGUAGE_EXPECTATIONS: readonly PopularLanguageExpectation[] = [
  {
    language: 'JavaScript',
    minifyExtensions: ['js', 'jsx', 'mjs', 'cjs'],
    symbolExtensions: ['js', 'jsx', 'mjs', 'cjs'],
  },
  {
    language: 'TypeScript',
    minifyExtensions: ['ts', 'tsx'],
    symbolExtensions: ['ts', 'tsx'],
  },
  {
    language: 'Python',
    minifyExtensions: ['py'],
    symbolExtensions: ['py'],
  },
  {
    language: 'Java',
    minifyExtensions: ['java'],
    symbolExtensions: ['java'],
  },
  {
    language: 'C#',
    minifyExtensions: ['cs'],
    symbolExtensions: ['cs'],
  },
  {
    language: 'Visual Basic',
    minifyExtensions: ['vb', 'vbs'],
    symbolExtensions: [],
  },
  {
    language: 'C',
    minifyExtensions: ['c', 'h'],
    symbolExtensions: ['c', 'h'],
  },
  {
    language: 'C++',
    minifyExtensions: ['cpp', 'hpp', 'cc'],
    symbolExtensions: ['cpp', 'hpp', 'cc'],
  },
  {
    language: 'Go',
    minifyExtensions: ['go'],
    symbolExtensions: ['go'],
  },
  {
    language: 'Rust',
    minifyExtensions: ['rs', 'rust'],
    symbolExtensions: ['rs', 'rust'],
  },
  {
    language: 'Kotlin',
    minifyExtensions: ['kt', 'kotlin'],
    symbolExtensions: ['kt', 'kotlin'],
  },
  {
    language: 'Swift',
    minifyExtensions: ['swift'],
    symbolExtensions: ['swift'],
  },
  {
    language: 'PHP',
    minifyExtensions: ['php'],
    symbolExtensions: ['php'],
  },
  {
    language: 'Ruby',
    minifyExtensions: ['rb'],
    symbolExtensions: ['rb'],
  },
  {
    language: 'Scala',
    minifyExtensions: ['scala'],
    symbolExtensions: ['scala'],
  },
  {
    language: 'Delphi/Object Pascal',
    minifyExtensions: ['pas'],
    symbolExtensions: [],
  },
  {
    language: 'Ada',
    minifyExtensions: ['adb', 'ads'],
    symbolExtensions: [],
  },
  {
    language: 'Fortran',
    minifyExtensions: ['f', 'for', 'f90', 'f95', 'f03', 'f08'],
    symbolExtensions: [],
  },
  {
    language: 'PowerShell',
    minifyExtensions: ['ps1', 'psm1', 'psd1'],
    symbolExtensions: [],
  },
  {
    language: 'Shell',
    minifyExtensions: ['sh', 'bash', 'zsh', 'fish'],
    symbolExtensions: ['sh', 'bash', 'zsh'],
  },
  {
    language: 'SQL',
    minifyExtensions: ['sql', 'tsql', 'plsql', 'pls', 'pks', 'pkb'],
    symbolExtensions: ['sql'],
  },
  {
    language: 'HTML',
    minifyExtensions: ['html', 'htm'],
    symbolExtensions: ['html', 'htm'],
  },
  {
    language: 'CSS',
    minifyExtensions: ['css', 'scss', 'less'],
    symbolExtensions: ['css', 'scss', 'less'],
  },
  {
    language: 'JSON family',
    minifyExtensions: ['json', 'jsonc', 'json5'],
    symbolExtensions: [],
  },
  {
    language: 'YAML',
    minifyExtensions: ['yaml', 'yml'],
    symbolExtensions: [],
  },
  {
    language: 'TOML and INI',
    minifyExtensions: ['toml', 'ini'],
    symbolExtensions: [],
  },
  {
    language: 'GraphQL',
    minifyExtensions: ['graphql', 'gql'],
    symbolExtensions: [],
  },
  {
    language: 'Protocol Buffers',
    minifyExtensions: ['proto'],
    symbolExtensions: [],
  },
  {
    language: 'Terraform',
    minifyExtensions: ['tf', 'tfvars'],
    symbolExtensions: [],
  },
  {
    language: 'Templates',
    minifyExtensions: [
      'hbs',
      'handlebars',
      'ejs',
      'pug',
      'jade',
      'mustache',
      'twig',
      'jinja',
      'jinja2',
      'erb',
    ],
    symbolExtensions: [],
  },
  {
    language: 'Plain text data',
    minifyExtensions: ['csv', 'txt', 'log'],
    symbolExtensions: [],
  },
  {
    language: 'Objective-C++',
    minifyExtensions: ['mm'],
    symbolExtensions: [],
  },
  {
    language: 'Zig',
    minifyExtensions: ['zig'],
    symbolExtensions: [],
  },
  {
    language: 'V or Verilog',
    minifyExtensions: ['v'],
    symbolExtensions: [],
  },
  {
    language: 'Julia',
    minifyExtensions: ['jl'],
    symbolExtensions: [],
  },
  {
    language: 'Nix',
    minifyExtensions: ['nix'],
    symbolExtensions: [],
  },
  {
    language: 'Groovy or Gradle',
    minifyExtensions: ['groovy', 'gradle'],
    symbolExtensions: [],
  },
  {
    language: 'WebAssembly text',
    minifyExtensions: ['wat', 'wast'],
    symbolExtensions: [],
  },
  {
    language: 'Other common source formats',
    minifyExtensions: [
      'xsl',
      'xslt',
      'awk',
      'lisp',
      'lsp',
      'scm',
      'rkt',
      'vhd',
      'vhdl',
      'asm',
      'nasm',
    ],
    symbolExtensions: [],
  },
  {
    language: 'Other common scripts',
    minifyExtensions: [
      'dart',
      'lua',
      'r',
      'perl',
      'pl',
      'pm',
      'erl',
      'hrl',
      'ex',
      'exs',
      'hs',
      'lhs',
      'fs',
      'fsx',
      'clj',
      'cljs',
      'elm',
    ],
    symbolExtensions: [],
  },
];

const INTENTIONALLY_UNSUPPORTED_EXTENSIONS = [
  'wasm',
  // .m has multiple incompatible source dialects; a single comment model is unsafe.
  'm',
  // COBOL and SAS comments are too grammar/column-sensitive for this scanner.
  'cob',
  'cbl',
  'sas',
  // ABAP and batch comments depend on statement position and command parsing.
  'abap',
  'bat',
  'cmd',
  // .s assembly comments vary by assembler and target dialect.
  's',
] as const;

describe('language coverage audit', () => {
  it('keeps synthetic benchmark fixtures aligned with every configured extension', () => {
    const configuredExtensions = Object.keys(MINIFY_CONFIG.fileTypes).sort();
    const benchmarkExtensions = buildLanguageBenchmarkCases()
      .map(testCase => testCase.ext)
      .sort();

    expect(benchmarkExtensions).toEqual(configuredExtensions);
  });

  it('keeps real benchmark discovery aligned with every configured extension', () => {
    expect([...COMMON_REAL_LANGUAGE_EXTENSIONS].sort()).toEqual(
      Object.keys(MINIFY_CONFIG.fileTypes).sort()
    );
  });

  it('does not expose symbols for extensions missing minify config', () => {
    const missingMinify = SUPPORTED_SIGNATURE_EXTENSIONS.filter(
      ext => !Object.hasOwn(MINIFY_CONFIG.fileTypes, ext)
    );

    expect(missingMinify).toEqual([]);
  });

  it.each(POPULAR_LANGUAGE_EXPECTATIONS)(
    '$language support is explicit and intentional',
    expectation => {
      for (const ext of expectation.minifyExtensions) {
        expect(
          MINIFY_CONFIG.fileTypes[ext],
          `${expectation.language} minify ${ext}`
        ).toBeDefined();
      }

      for (const ext of expectation.symbolExtensions) {
        expect(
          SUPPORTED_SIGNATURE_EXTENSIONS,
          `${expectation.language} symbols ${ext}`
        ).toContain(ext);
      }
    }
  );

  it('keeps extensionless popular build files covered by filename routing', () => {
    expect([...INDENTATION_SENSITIVE_NAMES]).toEqual(
      expect.arrayContaining(['makefile', 'dockerfile'])
    );
  });

  it('keeps binary or ambiguous popular extensions intentionally unsupported', () => {
    for (const ext of INTENTIONALLY_UNSUPPORTED_EXTENSIONS) {
      expect(MINIFY_CONFIG.fileTypes[ext]).toBeUndefined();
      expect(SUPPORTED_SIGNATURE_EXTENSIONS).not.toContain(ext);
    }
  });
});
