/**
 * Minimal terminal spinner — writes to stderr so stdout stays clean for --json.
 * No-op when stderr is not a TTY or CI=true.
 */

const FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

function isTTY(): boolean {
  return Boolean(process.stderr.isTTY) && !process.env['CI'] && !process.env['NO_COLOR'];
}

export class Spinner {
  private frame = 0;
  private text: string;
  private interval: ReturnType<typeof setInterval> | null = null;
  private active = false;
  private lastLen = 0;

  constructor(text: string) {
    this.text = text;
  }

  start(): this {
    if (!isTTY()) return this;
    this.active = true;
    this.interval = setInterval(() => {
      const f = FRAMES[this.frame % FRAMES.length]!;
      const line = `  ${f}  ${this.text}`;
      process.stderr.write(`\r${line}${' '.repeat(Math.max(0, this.lastLen - line.length))}`);
      this.lastLen = line.length;
      this.frame++;
    }, 80);
    return this;
  }

  update(text: string): this {
    this.text = text;
    return this;
  }

  private clear(): void {
    if (isTTY()) {
      process.stderr.write('\r' + ' '.repeat(this.lastLen + 2) + '\r');
    }
  }

  stop(): void {
    if (!this.active) return;
    if (this.interval) clearInterval(this.interval);
    this.clear();
    this.active = false;
  }

  succeed(text: string): void {
    this.stop();
    console.log(`  ✓  ${text}`);
  }

  fail(text: string): void {
    this.stop();
    console.log(`  ✗  ${text}`);
  }

  warn(text: string): void {
    this.stop();
    console.log(`  ⚠  ${text}`);
  }
}
