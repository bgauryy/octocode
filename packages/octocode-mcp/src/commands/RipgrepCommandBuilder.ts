import { BaseCommandBuilder } from './BaseCommandBuilder.js';
import { RESOURCE_LIMITS } from '../utils/core/constants.js';
import type { RipgrepQuery } from '@octocodeai/octocode-core';
import { resolveRipgrepBinary } from '../utils/exec/ripgrepBinary.js';

export class RipgrepCommandBuilder extends BaseCommandBuilder {
  constructor() {
    // T3.3 — prefer the bundled @vscode/ripgrep binary so the tool
    // works out-of-the-box without a system-wide ripgrep install.
    // Falls back to 'rg' (PATH lookup) when bundling failed.
    super(resolveRipgrepBinary());
  }

  /**
   * Simple convenience method to set pattern and path with default flags
   */
  simple(pattern: string, path: string): this {
    this.addFlag('-n');
    this.addFlag('--column');
    this.addFlag('-S'); // smart case by default
    this.addOption('--color', 'never');
    this.addOption('--sort', 'path');
    this.addArg('--');
    this.addArg(pattern);
    this.addArg(path);
    return this;
  }

  /**
   * Enable smart case sensitivity
   */
  smartCase(): this {
    this.addFlag('-S');
    return this;
  }

  /**
   * Only show filenames with matches
   */
  filesOnly(): this {
    this.addFlag('-l');
    return this;
  }

  /**
   * Show context lines around matches
   */
  context(lines: number): this {
    this.addOption('-C', lines);
    return this;
  }

  /**
   * Include only files matching glob pattern
   */
  include(pattern: string): this {
    this.addOption('-g', pattern);
    return this;
  }

  /**
   * Exclude files matching glob pattern
   */
  exclude(pattern: string): this {
    this.addOption('-g', `!${pattern}`);
    return this;
  }

  /**
   * Exclude directory from search
   */
  excludeDir(dir: string): this {
    this.addOption('-g', `!${dir}/`);
    return this;
  }

  /**
   * Filter by file type
   */
  type(fileType: string): this {
    this.addOption('-t', fileType);
    return this;
  }

  /**
   * Treat pattern as fixed string (not regex)
   */
  fixedString(): this {
    this.addFlag('-F');
    return this;
  }

  /**
   * Use Perl-compatible regex
   */
  perlRegex(): this {
    this.addFlag('-P');
    return this;
  }

  /**
   * Limit max matches per file
   */
  maxMatches(count: number): this {
    this.addOption('-m', count);
    return this;
  }

  fromQuery(query: RipgrepQuery): this {
    if (query.fixedString) {
      this.addFlag('-F');
    } else if (query.perlRegex) {
      this.addFlag('-P');
    }

    if (query.caseSensitive) {
      this.addFlag('-s');
    } else if (query.caseInsensitive) {
      this.addFlag('-i');
    } else if (query.smartCase !== false) {
      this.addFlag('-S');
    }

    if (query.noUnicode) {
      this.addFlag('--no-unicode');
    }

    if (query.encoding) {
      this.addOption('-E', query.encoding);
    }

    if (query.wholeWord) {
      this.addFlag('-w');
    }

    if (query.invertMatch) {
      this.addFlag('-v');
    }

    if (query.binaryFiles) {
      if (query.binaryFiles === 'text') {
        this.addFlag('-a');
      } else if (query.binaryFiles === 'binary') {
        this.addFlag('--binary');
      }
    }

    if (query.followSymlinks) {
      this.addFlag('-L');
    }

    if (query.contextLines !== undefined && query.contextLines > 0) {
      this.addOption('-C', query.contextLines);
    } else {
      if (query.beforeContext !== undefined && query.beforeContext > 0) {
        this.addOption('-B', query.beforeContext);
      }
      if (query.afterContext !== undefined && query.afterContext > 0) {
        this.addOption('-A', query.afterContext);
      }
    }

    this.addFlag('-n');
    this.addFlag('--column');

    if (query.filesOnly) {
      this.addFlag('-l');
    } else if (query.filesWithoutMatch) {
      this.addFlag('--files-without-match');
    } else if (query.countMatches) {
      this.addFlag('--count-matches');
    } else if (query.count) {
      this.addFlag('-c');
    }

    if (query.maxMatchesPerFile !== undefined) {
      this.addOption('-m', query.maxMatchesPerFile);
    } else if (!query.filesOnly && !query.count && !query.countMatches) {
      const limit = (
        query.matchesPerPage && Number.isFinite(query.matchesPerPage)
          ? query.matchesPerPage
          : RESOURCE_LIMITS.DEFAULT_MATCHES_PER_PAGE
      ) as number;
      this.addOption('-m', limit);
    }

    if (query.type) {
      this.addOption('-t', query.type);
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

    // Only add --json when NOT in plain text output modes
    // -l (filesOnly), --files-without-match, -c (count), and --count-matches
    // all output plain text (one item per line) — incompatible with or unnecessary for --json
    const isPlainTextOutput =
      query.filesOnly ||
      query.filesWithoutMatch ||
      query.count ||
      query.countMatches;
    if (!isPlainTextOutput) {
      this.addFlag('--json');
    }

    if (query.threads !== undefined) {
      this.addOption('-j', query.threads);
    }

    if (query.mmap === false) {
      this.addFlag('--no-mmap');
    }

    if (query.includeStats && !isPlainTextOutput) {
      this.addFlag('--stats');
    }

    const sortOption = query.sort || 'path';

    if (query.sortReverse) {
      this.clearSortOption();
      this.addOption('--sortr', sortOption);
    } else {
      this.clearSortrOption();
      this.addOption('--sort', sortOption);
    }

    this.addOption('--color', 'never');

    if (query.noMessages) {
      this.addFlag('--no-messages');
    }

    if (query.lineRegexp) {
      this.addFlag('-x');
    }

    if (query.passthru) {
      this.addFlag('--passthru');
    }

    if (query.debug) {
      this.addFlag('--debug');
    }

    // End option parsing so user-provided pattern/path cannot be interpreted as flags.
    this.addArg('--');
    this.addArg(query.pattern);
    this.addArg(query.path);

    return this;
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

  /**
   * Removes --sort option and its value if present
   * Used when switching to --sortr
   */
  private clearSortOption(): void {
    const sortIndex = this.args.indexOf('--sort');
    if (sortIndex !== -1 && sortIndex < this.args.length - 1) {
      this.args.splice(sortIndex, 2);
    }
  }

  /**
   * Removes --sortr option and its value if present
   * Used when switching to --sort
   */
  private clearSortrOption(): void {
    const sortrIndex = this.args.indexOf('--sortr');
    if (sortrIndex !== -1 && sortrIndex < this.args.length - 1) {
      this.args.splice(sortrIndex, 2);
    }
  }
}
