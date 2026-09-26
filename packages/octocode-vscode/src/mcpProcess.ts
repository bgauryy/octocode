import { spawn, type ChildProcess } from 'child_process';
import { MCP_ARGS, MCP_COMMAND, MCP_TOKEN_ENV } from './mcpConfig';

type ProcessEvents = {
  getToken(): Promise<string | undefined>;
  output(message: string): void;
  state(running: boolean): void;
  error(error: Error): void;
};

export class McpProcess {
  private child: ChildProcess | undefined;
  private starting = false;
  private generation = 0;

  constructor(private readonly events: ProcessEvents) {}

  get running(): boolean {
    return this.child !== undefined;
  }

  async start(): Promise<'started' | 'busy' | 'cancelled'> {
    if (this.child || this.starting) return 'busy';
    const generation = ++this.generation;
    this.starting = true;
    try {
      const token = await this.events.getToken();
      if (generation !== this.generation) return 'cancelled';
      const env = { ...process.env };
      if (token) env[MCP_TOKEN_ENV] = token;
      const child = spawn(MCP_COMMAND, [...MCP_ARGS], {
        env,
        stdio: ['pipe', 'pipe', 'pipe'],
        shell: process.platform === 'win32',
      });
      this.child = child;
      child.stdout?.on('data', (data: Buffer) =>
        this.events.output(`[stdout] ${data.toString()}`)
      );
      child.stderr?.on('data', (data: Buffer) =>
        this.events.output(`[stderr] ${data.toString()}`)
      );
      child.on('close', (code: number | null) => {
        if (this.child !== child) return;
        this.child = undefined;
        this.events.output(`MCP server exited with code ${code}`);
        this.events.state(false);
      });
      child.on('error', (error: Error) => {
        if (this.child !== child) return;
        this.child = undefined;
        this.events.state(false);
        this.events.error(error);
      });
      this.events.state(true);
      return 'started';
    } finally {
      if (generation === this.generation) this.starting = false;
    }
  }

  stop(): boolean {
    if (this.child && !this.child.kill()) {
      throw new Error('Failed to stop MCP server process');
    }
    const active = this.child !== undefined || this.starting;
    ++this.generation;
    this.child = undefined;
    this.starting = false;
    if (active) this.events.state(false);
    return active;
  }
}
