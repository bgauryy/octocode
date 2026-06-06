import { BaseCommandBuilder } from './BaseCommandBuilder.js';
import type { ViewStructureQuery } from '../scheme/localSchemaOverlay.js';

export class LsCommandBuilder extends BaseCommandBuilder {
  constructor() {
    super('ls');
  }

  fromQuery(
    query: Partial<ViewStructureQuery> & Pick<ViewStructureQuery, 'path'>
  ): this {
    this.addFlag('--color=never');

    if (process.platform === 'linux') {
      this.addFlag('--quoting-style=literal');
    }

    if (query.details) {
      this.addFlag('-l');
      if (process.platform === 'linux') {
        this.addFlag('--time-style=long-iso');
      }
    }

    if (query.hidden) {
      this.addFlag('-a');
    }

    if (query.humanReadable) {
      this.addFlag('-h');
    }

    if (query.recursive) {
      this.addFlag('-R');
    }

    if (query.reverse) {
      this.addFlag('-r');
    }

    if (query.sortBy) {
      switch (query.sortBy) {
        case 'size':
          this.addFlag('-S');
          break;
        case 'time':
          this.addFlag('-t');
          break;
        case 'extension':
          this.addFlag('-X');
          break;
        case 'name':
        default:
          break;
      }
    }

    if (!query.sortBy || query.sortBy === 'name') {
      if (process.platform === 'linux') {
        this.addFlag('--group-directories-first');
      }
    }

    if (!query.details) {
      this.addFlag('-1');
    }

    this.addArg('--');
    this.addArg(query.path);

    return this;
  }

  simple(path: string): this {
    this.addArg('--');
    this.addArg(path);
    return this;
  }

  detailed(): this {
    this.addFlag('-l');
    return this;
  }

  all(): this {
    this.addFlag('-a');
    return this;
  }

  humanReadable(): this {
    this.addFlag('-h');
    return this;
  }

  recursive(): this {
    this.addFlag('-R');
    return this;
  }

  sortBySize(): this {
    this.addFlag('-S');
    return this;
  }

  sortByTime(): this {
    this.addFlag('-t');
    return this;
  }

  reverse(): this {
    this.addFlag('-r');
    return this;
  }

  path(path: string): this {
    this.addArg('--');
    this.addArg(path);
    return this;
  }
}
