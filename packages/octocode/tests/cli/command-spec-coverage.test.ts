import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

import {
  REGISTERED_COMMAND_NAMES,
  loadCommand,
} from '../../src/cli/commands/index.js';
import { findStaticCommandHelp } from '../../src/cli/command-help-specs.js';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

// Single source of truth: every CLI command's human-facing spec
// (description / usage / scheme / whenToUse / examples) lives in octocode-core
// and is resolved by name via findStaticCommandHelp. Command files carry only
// behavior (name + options + handler). These tests enforce that the content
// genuinely comes from core — so it can never silently drift back into a
// hardcoded string in the CLI package.
describe('CLI command content is sourced from octocode-core', () => {
  it('every registered command resolves a spec from core', () => {
    const missing = REGISTERED_COMMAND_NAMES.filter(
      name => !findStaticCommandHelp(name)
    );
    expect(missing).toEqual([]);
  });

  it('each resolved core spec carries the required help content', () => {
    for (const name of REGISTERED_COMMAND_NAMES) {
      const spec = findStaticCommandHelp(name);
      expect(spec, `no core spec for "${name}"`).toBeDefined();
      expect(spec!.description.trim().length).toBeGreaterThan(0);
      expect(spec!.usage?.startsWith(spec!.name)).toBe(true);
    }
  });

  it('command files no longer hardcode description/usage (only core has them)', async () => {
    // Spot-check a representative command object: it should expose name +
    // options + handler, but NOT a description/usage of its own.
    const { searchCommand } = await import('../../src/cli/commands/search.js');
    const obj = searchCommand as unknown as Record<string, unknown>;
    expect(obj.name).toBe('search');
    expect(typeof obj.handler).toBe('function');
    expect(obj.description).toBeUndefined();
    expect(obj.usage).toBeUndefined();
  });

  it('every runtime option is documented in its core spec (no undocumented flags)', async () => {
    // The runtime CLICommand.options and the core spec.options are independent
    // lists. getAllowedOptionNames() unions them, so a flag the handler reads
    // but the core spec omits is silently accepted yet never shown in --help
    // (the cache --clone/--tree regression). Enforce that every
    // runtime option name exists in core, and that hasValue agrees where both
    // declare it — so help can never under-document a working flag again.
    const offenders: string[] = [];

    // Flags shipped ahead of an octocode-core spec release. Each entry is
    // KNOWN missing from --help until core documents it — remove the entry
    // the moment the core spec catches up so the guard re-arms.
    const pendingCoreSpec = new Set([
      'search:quiet', // rows-only token-frugal mode (benchmark fix P0-2)
    ]);

    for (const name of REGISTERED_COMMAND_NAMES) {
      const command = await loadCommand(name);
      if (!command) {
        offenders.push(`${name}: runtime command failed to load`);
        continue;
      }
      const spec = findStaticCommandHelp(name);
      if (!spec) {
        offenders.push(`${name}: no core spec`);
        continue;
      }
      const coreByName = new Map(
        (spec.options ?? []).map(opt => [opt.name, opt])
      );
      for (const opt of command.options ?? []) {
        const core = coreByName.get(opt.name);
        if (!core) {
          if (pendingCoreSpec.has(`${name}:${opt.name}`)) continue;
          offenders.push(`${name}: --${opt.name} not documented in core spec`);
          continue;
        }
        if (Boolean(core.hasValue) !== Boolean(opt.hasValue)) {
          offenders.push(
            `${name}: --${opt.name} hasValue mismatch (runtime=${Boolean(opt.hasValue)}, core=${Boolean(core.hasValue)})`
          );
        }
      }
    }

    expect(offenders).toEqual([]);
  });

  it('runtime command options carry no local descriptions', () => {
    const commandDir = join(packageRoot, 'src/cli/commands');
    const files = [
      ...readdirSync(commandDir)
        .filter(file => file.endsWith('.ts'))
        .map(file => join(commandDir, file)),
      join(packageRoot, 'src/cli/tool-command.ts'),
    ];
    const offenders: string[] = [];

    function propName(prop: ts.ObjectLiteralElementLike): string | undefined {
      const name = prop.name;
      if (!name) return undefined;
      return ts.isIdentifier(name) || ts.isStringLiteral(name)
        ? name.text
        : undefined;
    }

    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      const parsed = ts.createSourceFile(
        file,
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.TS
      );

      function visit(node: ts.Node): void {
        if (
          ts.isPropertyAssignment(node) &&
          propName(node) === 'options' &&
          ts.isArrayLiteralExpression(node.initializer)
        ) {
          for (const option of node.initializer.elements) {
            if (!ts.isObjectLiteralExpression(option)) continue;
            const hasDescription = option.properties.some(
              prop =>
                ts.isPropertyAssignment(prop) &&
                propName(prop) === 'description'
            );
            if (hasDescription) {
              offenders.push(
                `${file}:${parsed.getLineAndCharacterOfPosition(option.getStart(parsed)).line + 1}`
              );
            }
          }
        }
        ts.forEachChild(node, visit);
      }
      visit(parsed);
    }

    expect(offenders).toEqual([]);
  });
});
