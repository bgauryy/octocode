import { EventEmitter } from 'events';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { McpProcess } from '../src/mcpProcess';

const spawn = vi.hoisted(() => vi.fn());
vi.mock('child_process', () => ({ spawn }));
const events = {
  getToken: vi.fn(),
  output: vi.fn(),
  state: vi.fn(),
  error: vi.fn(),
};
let process: McpProcess;
function child() {
  return Object.assign(new EventEmitter(), {
    stdout: new EventEmitter(),
    stderr: new EventEmitter(),
    kill: vi.fn(() => true),
  });
}
beforeEach(() => {
  spawn.mockReset();
  events.getToken.mockReset().mockResolvedValue('token');
  process = new McpProcess(events);
});

describe('MCP process lifecycle', () => {
  it('passes credentials and noninteractive arguments and forwards output', async () => {
    const current = child();
    spawn.mockReturnValue(current);
    expect(await process.start()).toBe('started');
    expect(spawn).toHaveBeenCalledWith(
      'npx',
      ['-y', 'octocode-mcp@latest'],
      expect.objectContaining({
        env: expect.objectContaining({ OCTOCODE_TOKEN: 'token' }),
      })
    );
    current.stdout.emit('data', Buffer.from('out'));
    current.stderr.emit('data', Buffer.from('err'));
    expect(events.output).toHaveBeenCalledWith('[stdout] out');
    expect(events.output).toHaveBeenCalledWith('[stderr] err');
    current.emit('close', 1);
    expect(process.running).toBe(false);
    expect(events.state).toHaveBeenLastCalledWith(false);
  });

  it('cancels a pending start when stopped', async () => {
    let resolve!: (value: undefined) => void;
    events.getToken.mockReturnValue(
      new Promise(done => {
        resolve = done;
      })
    );
    const pending = process.start();
    expect(process.stop()).toBe(true);
    resolve(undefined);
    expect(await pending).toBe('cancelled');
    expect(spawn).not.toHaveBeenCalled();
    expect(process.stop()).toBe(false);
  });

  it('allows retry after synchronous startup failure', async () => {
    spawn.mockImplementationOnce(() => {
      throw new Error('spawn failed');
    });
    await expect(process.start()).rejects.toThrow('spawn failed');
    spawn.mockReturnValue(child());
    expect(await process.start()).toBe('started');
    expect(await process.start()).toBe('busy');
  });

  it('handles process errors and ignores stale errors after restarting', async () => {
    const first = child();
    const second = child();
    spawn.mockReturnValueOnce(first).mockReturnValueOnce(second);
    await process.start();
    const error = new Error('process failed');
    first.emit('error', error);
    expect(events.error).toHaveBeenCalledWith(error);
    expect(process.running).toBe(false);
    await process.start();
    first.emit('error', error);
    first.emit('close', 1);
    expect(process.running).toBe(true);
    expect(events.error).toHaveBeenCalledOnce();
  });

  it('retains ownership if killing the child fails', async () => {
    const current = child();
    spawn.mockReturnValue(current);
    await process.start();
    current.kill.mockReturnValue(false);
    expect(() => process.stop()).toThrow('Failed to stop');
    expect(process.running).toBe(true);
    current.kill.mockReturnValue(true);
    expect(process.stop()).toBe(true);
  });
});
