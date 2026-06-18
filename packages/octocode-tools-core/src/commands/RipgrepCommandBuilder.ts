import { BaseCommandBuilder } from './BaseCommandBuilder.js';
import { resolveRipgrepBinary } from '../utils/exec/ripgrepBinary.js';
import type { RipgrepQuery } from '../tools/local_ripgrep/scheme.js';

export class RipgrepCommandBuilder extends BaseCommandBuilder {
  constructor() {
    super(resolveRipgrepBinary());
    // Never read ripgrep config files. This makes output deterministic
    // (a user's global rg config can't alter flags we parse) and closes the
    // RIPGREP_CONFIG_PATH vector — a config file can inject flags like --pre,
    // which would run an arbitrary command per file, bypassing our allowlist.
    this.addFlag('--no-config');
  }

  simple(pattern: string, path: string): this {
    this.addFlag('-n');
    this.addFlag('--column');
    this.addFlag('-S');
    this.addOption('--color', 'never');
    this.addOption('--sort', 'path');
    this.addArg('--');
    this.addArg(pattern);
    this.addArg(path);
    return this;
  }

  filesOnly(): this {
    this.addFlag('-l');
    return this;
  }

  context(lines: number): this {
    this.addOption('-C', lines);
    return this;
  }

  include(pattern: string): this {
    this.addOption('-g', pattern);
    return this;
  }

  exclude(pattern: string): this {
    this.addOption('-g', `!${pattern}`);
    return this;
  }

  excludeDir(dir: string): this {
    this.addOption('-g', `!${dir}/`);
    return this;
  }

  type(fileType: string): this {
    this.addOption('-t', fileType);
    return this;
  }

  fixedString(): this {
    this.addFlag('-F');
    return this;
  }

  perlRegex(): this {
    this.addFlag('-P');
    return this;
  }

  maxMatches(count: number): this {
    this.addOption('-m', count);
    return this;
  }

  fromQuery(query: RipgrepQuery): this {
    this._applyMatchFlags(query);
    this._applyContextFlags(query);

    this.addFlag('-n');
    this.addFlag('--column');

    this._applyOutputModeFlags(query);
    this._applyFilterFlags(query);

    const isPlainTextOutput = this._isPlainTextOutput(query);
    if (!isPlainTextOutput) {
      this.addFlag('--json');
    }

    this._applyExecutionFlags();
    this._applySortFlags(query);
    this._applyDiagnosticFlags(query);

    this.addArg('--');
    // keywords is required for every non-structural mode (schema-enforced);
    // structural search never builds an rg command.
    this.addArg(query.keywords ?? '');
    this.addArg(query.path);

    return this;
  }

  private _isPlainTextOutput(query: RipgrepQuery): boolean {
    return !!(
      query.filesOnly ||
      query.filesWithoutMatch ||
      query.countLinesPerFile ||
      query.countMatchesPerFile
    );
  }

  private _applyMatchFlags(query: RipgrepQuery): void {
    if (query.fixedString) {
      this.addFlag('-F');
    } else if (query.perlRegex) {
      this.addFlag('-P');
    }

    if (query.caseSensitive) {
      this.addFlag('-s');
    } else if (query.caseInsensitive) {
      this.addFlag('-i');
    } else {
      this.addFlag('-S');
    }

    if (query.wholeWord) {
      this.addFlag('-w');
    }

    if (query.invertMatch) {
      this.addFlag('-v');
    }
  }

  private _applyContextFlags(query: RipgrepQuery): void {
    if (query.contextLines !== undefined && query.contextLines > 0) {
      this.addOption('-C', query.contextLines);
    }
  }

  private _applyOutputModeFlags(query: RipgrepQuery): void {
    if (query.filesOnly) {
      this.addFlag('-l');
    } else if (query.filesWithoutMatch) {
      this.addFlag('--files-without-match');
    } else if (query.countMatchesPerFile) {
      this.addFlag('--count-matches');
    } else if (query.countLinesPerFile) {
      this.addFlag('-c');
    }

    void query;
  }

  private _applyFilterFlags(query: RipgrepQuery): void {
    if (query.langType) {
      this.addOption('-t', query.langType);
    }

    if (query.include && query.include.length > 0) {
      const consolidatedGlobs = this._consolidateGlobs(query.include);

      for (const glob of consolidatedGlobs) {
        this.addOption('-g', glob);
      }
    }

    if (query.exclude && query.exclude.length > 0) {
      for (const pattern of query.exclude) {
        this.addOption('-g', `!${pattern}`);
      }
    }

    if (query.excludeDir && query.excludeDir.length > 0) {
      for (const dir of query.excludeDir) {
        this.addOption('-g', `!${dir}/`);
      }
    }

    if (query.noIgnore) {
      this.addFlag('--no-ignore');
    }

    if (query.hidden) {
      this.addFlag('--hidden');
    }

    if (query.multiline) {
      this.addFlag('-U');

      if (query.multilineDotall) {
        this.addFlag('--multiline-dotall');
      }
    }
  }

  private _applyExecutionFlags(): void {
    const MAX_MCP_THREADS = 4;
    this.addOption('-j', MAX_MCP_THREADS);
  }

  private _applySortFlags(query: RipgrepQuery): void {
    const sortOption = query.sort || 'path';

    if (query.sortReverse) {
      this.clearSortOption();
      this.addOption('--sortr', sortOption);
    } else {
      this.clearSortrOption();
      this.addOption('--sort', sortOption);
    }
  }

  private _applyDiagnosticFlags(query: RipgrepQuery): void {
    this.addOption('--color', 'never');
    void query;
  }

  private _consolidateGlobs(globs: string[]): string[] {
    const simpleGlobPattern = /^\*\.([a-zA-Z0-9]+)$/;

    const simpleGlobs: string[] = [];
    const complexGlobs: string[] = [];

    for (const glob of globs) {
      const match = glob.match(simpleGlobPattern);
      if (match && match[1]) {
        simpleGlobs.push(match[1]);
      } else {
        complexGlobs.push(glob);
      }
    }

    const result: string[] = [];

    if (simpleGlobs.length > 1) {
      result.push(`*.{${simpleGlobs.join(',')}}`);
    } else if (simpleGlobs.length === 1) {
      result.push(`*.${simpleGlobs[0]}`);
    }

    result.push(...complexGlobs);

    return result;
  }

  private clearSortOption(): void {
    const sortIndex = this.args.indexOf('--sort');
    if (sortIndex !== -1 && sortIndex < this.args.length - 1) {
      this.args.splice(sortIndex, 2);
    }
  }

  private clearSortrOption(): void {
    const sortrIndex = this.args.indexOf('--sortr');
    if (sortrIndex !== -1 && sortrIndex < this.args.length - 1) {
      this.args.splice(sortrIndex, 2);
    }
  }
}
