import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { describe, expect, it } from 'vitest';
import { BrowserTool, registerBrowserTool } from '../src/browser/tool.js';

type Schema = { properties: Record<string, { type?: string; description?: string; items?: Schema }> };

describe('browser tool schema', () => {
  it('takes element numbers as integers and explains text per action', () => {
    let parameters: Schema | undefined;
    const pi = { registerTool: (tool: { parameters: Schema }) => (parameters = tool.parameters), on: () => undefined } as unknown as ExtensionAPI;
    registerBrowserTool(pi, new BrowserTool());
    const props = parameters!.properties;
    expect(props['ref']!.type).toBe('integer');
    expect(props['to']!.type).toBe('integer');
    expect(props['fields']!.items!.properties['ref']!.type).toBe('integer');
    for (const action of ['type:', 'wait:', 'navigate/snapshot:', 'drag:', 'dialog:']) expect(props['text']!.description).toContain(action);
  });
});
