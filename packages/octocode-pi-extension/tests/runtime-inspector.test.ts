import { openScrollInspector } from '../src/tui/scroll-inspector.js';
import { describe, expect, it, vi } from 'vitest';
vi.mock('../src/tools/runtime-renderer.js', () => ({
  runtimeStoreFor: () => undefined,
}));
import {
  executionStatusLines,
  registerRuntimeInspectors,
} from '../src/tui/runtime-inspector.js';
import { readExecutionEvents } from '../src/tools/execution-runtime.js';
import { visibleWidth } from '../src/tui/width.js';
import type { PiContext, PiInstance } from '../src/types.js';
import {
  createExecutionState,
  EXECUTION_ENTRY_TYPE,
} from '../src/tools/execution-events.js';

describe('runtime inspector', () => {
  it('warns RPC hosts instead of silently invoking a terminal-only custom view', async () => {
    const custom = vi.fn(async () => undefined);
    const notify = vi.fn();
    await openScrollInspector({ mode: 'rpc', hasUI: true, ui: { custom, notify } } as PiContext, 'Status', ['ready']);
    expect(custom).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith('This view needs an interactive terminal.', 'warning');
  });

  it('does not fill an idle session with empty feature sections', () => {
    const lines = executionStatusLines(createExecutionState());
    for (const title of ['Needs you', 'Tools', 'Skills', 'Plan', 'Agents', 'File operations']) {
      expect(lines).not.toContain(title);
    }
  });

  it('filters before wrapping and restores the complete view when search is cleared', async () => {
    let component: any;
    const done = vi.fn();
    const ctx = { mode: 'tui', hasUI: true, ui: { custom: (factory: any) => {
      component = factory({ terminal: { rows: 12 }, requestRender: vi.fn() }, undefined, {}, done);
    } } } as unknown as PiContext;
    await openScrollInspector(ctx, 'Status', ['tool success · parser', 'worker blocked · renderer', 'worker failed · parser']);
    component.render(36);
    component.handleInput('/');
    component.handleInput('parser');
    component.handleInput('\r');
    const filtered = component.render(36).join('\n');
    expect(filtered).toContain('tool success');
    expect(filtered).toContain('worker failed');
    expect(filtered).not.toContain('renderer');
    component.handleInput('\x1b');
    expect(component.render(36).join('\n')).toContain('renderer');
    expect(done).not.toHaveBeenCalled();
    component.handleInput('\x1b');
    expect(done).toHaveBeenCalledOnce();
  });
  it('shows fractional provider cost and rejects corrupt entries instead of exporting a partial journal', async () => {
    expect(
      executionStatusLines({
        ...createExecutionState(),
        usage: { cost: 0.005 },
      }).join('\n')
    ).toContain('$0.005000');
    const notify = vi.fn();
    const ctx = {
      ui: { notify },
      sessionManager: {
        getBranch: () => [
          {
            type: 'custom',
            customType: EXECUTION_ENTRY_TYPE,
            data: { type: 'broken' },
          },
        ],
      },
    } as unknown as PiContext;
    expect(() => readExecutionEvents(ctx)).toThrow(/invalid execution event/);
    const commands = new Map<string, any>();
    registerRuntimeInspectors({
      registerCommand: (name: string, command: any) =>
        commands.set(name, command),
    } as unknown as PiInstance);
    await commands.get('octocode-status').handler('export', ctx);
    expect(notify).toHaveBeenCalledWith(
      expect.stringContaining('invalid execution event'),
      'error'
    );
  });
  it.each([36, 52, 80, 120, 160])(
    'keeps every line reachable at %i columns with scrolling and escape',
    async width => {
      let component: {
        render(width: number): string[];
        handleInput(data: string): void;
      };
      const done = vi.fn();
      const render = vi.fn();
      const theme = {
        fg: (_token: string, text: string) => text,
        bold: (text: string) => text,
      };
      const ctx = {
        hasUI: true,
        mode: 'tui',
        ui: {
          custom: (factory: any) => {
            component = factory(
              { terminal: { rows: 18 }, requestRender: render },
              theme,
              {},
              done
            );
          },
        },
      } as unknown as PiContext;
      const lines = Array.from(
        { length: 70 },
        (_, index) => `event ${index} ${'中文'.repeat(8)}`
      );
      await openScrollInspector(ctx, 'Events', lines);
      const seen = new Set<string>();
      for (let index = 0; index < 160; index++) {
        const page = component!.render(width);
        expect(page.length).toBeLessThanOrEqual(14);
        for (const line of page) {
          expect(visibleWidth(line)).toBeLessThanOrEqual(width);
          seen.add(line);
        }
        component!.handleInput('\u001b[B');
      }
      for (let index = 0; index < 70; index++)
        expect([...seen].some(line => line.startsWith(`event ${index} `))).toBe(
          true
        );
      component!.handleInput('g');
      expect(component!.render(width)[1]).toContain('event 0 ');
      component!.handleInput('G');
      expect(component!.render(width).join('\n')).toContain('event 69 ');
      component!.handleInput('\u001b');
      expect(done).toHaveBeenCalledOnce();
    }
  );

  it('uses one inspection command for status, events, and export and reads only the selected branch', () => {
    const registerCommand = vi.fn();
    registerRuntimeInspectors({ registerCommand } as unknown as PiInstance);
    expect(registerCommand.mock.calls.map(call => call[0])).toEqual([
      'octocode-status',
    ]);
    expect(
      readExecutionEvents({
        sessionManager: {
          getBranch: () => [
            { type: 'message', message: { content: 'private' } },
          ],
        },
      } as unknown as PiContext)
    ).toEqual([]);
  });
});
