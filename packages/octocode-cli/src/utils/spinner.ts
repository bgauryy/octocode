import { c } from './colors.js';
import type { ColorName } from '../types/index.js';

const activeSpinners = new Set<Spinner>();

function ensureCursorRestored(): void {
  process.stdout.write('\x1B[?25h');
}

let cleanupRegistered = false;
function registerCleanupHandlers(): void {
  if (cleanupRegistered) return;
  cleanupRegistered = true;

  process.on('exit', ensureCursorRestored);
  process.on('SIGINT', () => {
    ensureCursorRestored();
    process.exit(0);
  });
  process.on('SIGTERM', () => {
    ensureCursorRestored();
    process.exit(0);
  });
  process.on('uncaughtException', err => {
    ensureCursorRestored();
    console.error('Uncaught exception:', err);
    process.exit(1);
  });
}

function spinnerEnabled(): boolean {
  return Boolean(process.stdout.isTTY);
}

export class Spinner {
  private text: string;
  private frames: string[];
  private i: number;
  private timer: NodeJS.Timeout | null;

  constructor(text: string = '') {
    this.text = text;
    this.frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    this.i = 0;
    this.timer = null;
  }

  start(text?: string, indent: number = 0): this {
    if (text) this.text = text;

    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }

    if (!spinnerEnabled()) {
      return this;
    }

    registerCleanupHandlers();

    activeSpinners.add(this);

    process.stdout.write('\x1B[?25l');

    const indentStr = ' '.repeat(indent);

    this.timer = setInterval(() => {
      const frame = this.frames[this.i++ % this.frames.length];

      process.stdout.write(`\r${indentStr}${c('cyan', frame)} ${this.text}`);
    }, 80);

    return this;
  }

  clear(): this {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }

    activeSpinners.delete(this);

    if (!spinnerEnabled()) {
      return this;
    }

    process.stdout.write('\r\x1B[2K');

    process.stdout.write('\x1B[?25h');

    return this;
  }

  stop(symbol: string = '✓', color: ColorName = 'green'): this {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }

    activeSpinners.delete(this);

    if (!spinnerEnabled()) {
      process.stdout.write(`${c(color, symbol)} ${this.text}\n`);
      return this;
    }

    process.stdout.write(`\r\x1B[2K${c(color, symbol)} ${this.text}\n`);

    process.stdout.write('\x1B[?25h');

    return this;
  }

  succeed(text?: string): this {
    if (text) this.text = text;
    return this.stop('✓', 'green');
  }

  fail(text?: string): this {
    if (text) this.text = text;
    return this.stop('✗', 'red');
  }

  info(text?: string): this {
    if (text) this.text = text;
    return this.stop('ℹ', 'blue');
  }

  warn(text?: string): this {
    if (text) this.text = text;
    return this.stop('⚠', 'yellow');
  }

  update(text: string): this {
    this.text = text;
    return this;
  }
}
