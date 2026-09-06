import { describe, expect, it } from 'vitest';
import { createRustModuleResolver } from '../../../src/graph/rustModules.js';
import type { RawGraphFacts } from '../../../src/graph/types.js';

function resolver(
  entries: Record<string, RawGraphFacts>,
  roots = ['src/lib.rs']
) {
  return createRustModuleResolver(
    new Map(Object.entries(entries)),
    new Set(Object.keys(entries)),
    roots
  );
}

describe('explicit Rust module forest', () => {
  it.each([
    [
      'super::language::AgLanguage',
      'src/structural/files.rs',
      'src/structural/language.rs',
    ],
    [
      'super::query::Prefilter',
      'src/structural/files.rs',
      'src/structural/query.rs',
    ],
    ['crate::types::Options', 'src/structural/files.rs', 'src/types.rs'],
    ['self::types::Thing', 'src/structural/mod.rs', 'src/structural/types.rs'],
    ['super::types::*', 'src/structural/files.rs', 'src/structural/types.rs'],
    ['super::super::types::Thing', 'src/structural/files.rs', 'src/types.rs'],
  ])('links declared module path %s from %s', (specifier, file, target) => {
    const modules = (names: string[]) =>
      names.map((name, index) => ({
        name,
        line: index + 1,
        scope: [],
        inline: false,
      }));
    const resolve = resolver({
      'src/lib.rs': { modules: modules(['types', 'structural']) },
      'src/types.rs': {},
      'src/structural/mod.rs': {
        modules: modules(['files', 'language', 'query', 'types']),
      },
      'src/structural/files.rs': {},
      'src/structural/language.rs': {},
      'src/structural/query.rs': {},
      'src/structural/types.rs': {},
    });
    expect(resolve(specifier, file)).toEqual({ status: 'resolved', target });
  });

  it('rejects missing declarations, malformed paths, and unmodelled crate roots', () => {
    const resolve = resolver({
      'src/lib.rs': {},
      'src/bin/tool.rs': {},
      'src/bin/helper.rs': {},
    });
    expect(resolve('self::missing', 'src/lib.rs', [], true).status).toBe(
      'unresolvedInternal'
    );
    expect(resolve('crate::missing::Thing', 'src/lib.rs').status).toBe(
      'unresolvedInternal'
    );
    expect(resolve('std::fs', 'src/lib.rs').status).toBe('external');
    expect(resolve('other::Thing', 'src/lib.rs').status).toBe('unsupported');
    expect(resolve('crate::types::{Thing}', 'src/lib.rs').status).toBe(
      'unsupported'
    );
    expect(resolve('crate::Thing', 'files.rs').status).toBe('unsupported');
    for (const specifier of [
      'crate::Thing',
      'crate::shared::Thing',
      'self::helper',
    ]) {
      expect(resolve(specifier, 'src/bin/tool.rs').status).toBe('unsupported');
    }
  });

  it('resolves a declared bare root module and respects known edition scopes', () => {
    const facts = new Map<string, RawGraphFacts>([
      [
        'src/lib.rs',
        {
          modules: [
            { name: 'child', line: 1, scope: [], inline: false },
            { name: 'nested', line: 2, scope: [], inline: true },
            { name: 'child', line: 3, scope: ['nested'], inline: false },
          ],
        },
      ],
      ['src/child.rs', {}],
      ['src/nested/child.rs', {}],
    ]);
    const files = new Set(facts.keys());
    const modern = createRustModuleResolver(
      facts,
      files,
      ['src/lib.rs'],
      new Map(),
      new Map([['src/lib.rs', '2021']])
    );
    expect(modern('child::Thing', 'src/lib.rs').target).toBe('src/child.rs');
    expect(modern('child::Thing', 'src/lib.rs', ['nested']).target).toBe(
      'src/nested/child.rs'
    );
    const old = createRustModuleResolver(
      facts,
      files,
      ['src/lib.rs'],
      new Map(),
      new Map([['src/lib.rs', '2015']])
    );
    expect(old('child::Thing', 'src/lib.rs', ['nested']).target).toBe(
      'src/child.rs'
    );
    const unknown = createRustModuleResolver(facts, files, ['src/lib.rs']);
    expect(unknown('child::Thing', 'src/lib.rs', ['nested']).status).toBe(
      'unsupported'
    );
  });

  it('does not resolve through skipped or cfg-gated module facts', () => {
    const facts = new Map<string, RawGraphFacts>([
      [
        'src/lib.rs',
        { modules: [{ name: 'child', line: 1, scope: [], inline: false }] },
      ],
    ]);
    const files = new Set(['src/lib.rs', 'src/child.rs']);
    expect(
      createRustModuleResolver(facts, files, ['src/lib.rs'])(
        'crate::child::Thing',
        'src/lib.rs'
      ).status
    ).toBe('unsupported');
    facts.set('src/child.rs', { rustRootUnsupported: true });
    expect(
      createRustModuleResolver(facts, files, ['src/lib.rs'])(
        'crate::child::Thing',
        'src/lib.rs'
      ).status
    ).toBe('unsupported');
  });
  it('resolves literal path aliases and crate imports to declared physical files', () => {
    const resolve = resolver({
      'src/lib.rs': {
        modules: [
          {
            name: 'alias',
            line: 2,
            scope: [],
            inline: false,
            path: 'actual.rs',
          },
          { name: 'consumer', line: 3, scope: [], inline: false },
        ],
      },
      'src/actual.rs': {
        modules: [{ name: 'nested', line: 1, scope: [], inline: false }],
      },
      'src/nested.rs': {},
      'src/consumer.rs': {},
      'src/alias.rs': {},
    });
    expect(resolve('crate::alias::Thing', 'src/consumer.rs').target).toBe(
      'src/actual.rs'
    );
    expect(
      resolve('crate::alias::nested::Thing', 'src/consumer.rs').target
    ).toBe('src/nested.rs');
    expect(resolve('self::alias', 'src/lib.rs', [], true, 2).target).toBe(
      'src/actual.rs'
    );
  });

  it('resolves inline paths relative to mod-rs and non-mod-rs owners', () => {
    const resolve = resolver({
      'src/lib.rs': {
        modules: [{ name: 'outer', line: 1, scope: [], inline: false }],
      },
      'src/outer.rs': {
        modules: [
          { name: 'inline', line: 1, scope: [], inline: true },
          {
            name: 'child',
            line: 2,
            scope: ['inline'],
            inline: false,
            path: 'actual.rs',
          },
        ],
      },
      'src/outer/inline/actual.rs': {},
    });
    expect(
      resolve('self::child::Thing', 'src/outer.rs', ['inline']).target
    ).toBe('src/outer/inline/actual.rs');
    expect(resolve('super::Thing', 'src/outer/inline/actual.rs').target).toBe(
      'src/outer.rs'
    );
  });

  it('does not invent undeclared modules or pick between conditional alternatives', () => {
    const resolve = resolver({
      'src/lib.rs': {
        modules: [
          {
            name: 'maybe',
            line: 1,
            scope: [],
            inline: false,
            unsupported: true,
          },
        ],
      },
      'src/maybe.rs': {},
      'src/undeclared.rs': {},
    });
    expect(resolve('crate::maybe::Thing', 'src/lib.rs').status).toBe(
      'unsupported'
    );
    expect(resolve('crate::undeclared::Thing', 'src/lib.rs').status).toBe(
      'unresolvedInternal'
    );
  });

  it('uses explicit alternate roots without borrowing the neighboring library', () => {
    const resolve = resolver(
      {
        'src/lib.rs': {},
        'src/bin/tool.rs': {
          modules: [{ name: 'helper', line: 1, scope: [], inline: false }],
        },
        'src/bin/helper.rs': {},
      },
      ['src/lib.rs', 'src/bin/tool.rs']
    );
    expect(resolve('crate::helper::Thing', 'src/bin/tool.rs').target).toBe(
      'src/bin/helper.rs'
    );
    expect(resolve('super::Thing', 'src/bin/helper.rs').target).toBe(
      'src/bin/tool.rs'
    );
  });
});
