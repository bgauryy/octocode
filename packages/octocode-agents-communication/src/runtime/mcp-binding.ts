/** One connection's pending, externally owned or managed coordination identity. */
import * as catalog from './catalog.js';
import { databasePath } from './config.js';
import { ValidationError } from './errors.js';
import { claimDeliveryOwner } from './owner-lock.js';
import { initialize } from './sessions.js';
import { Store } from './store.js';
import { isBlank } from './text.js';
import { mcpInstructions } from './mcp-instructions.js';

export interface McpArgs {
  session?: string;
  database?: string;
  workspace: string;
  vendor?: string;
  name?: string;
  tools?: string;
  managed: boolean;
}

export class McpBinding {
  store: Store | null = null;
  session = '';
  managed = false;
  private owner: Awaited<ReturnType<typeof claimDeliveryOwner>> | null = null;
  private leave = false;

  constructor(private readonly args: McpArgs) {}

  private open(create: boolean): Store {
    return this.store = new Store(databasePath(this.args.database), this.args.workspace, false, create);
  }

  async start(): Promise<void> {
    if (!this.args.managed) {
      if (this.args.session) {
        const store = this.open(false);
        this.session = store.known(this.args.session).id;
      }
      return;
    }
    const vendor = this.args.vendor;
    if (vendor === undefined || isBlank(vendor)) throw new ValidationError('--vendor required for managed MCP');
    const create = !this.args.session;
    const input = { name: this.args.name ?? '', vendor };
    if (create) catalog.command('join', input);
    const store = this.open(create);
    if (this.args.session) {
      this.session = this.args.session;
      if (store.known(this.session, false).vendor !== vendor) throw new ValidationError('Vendor mismatch for managed MCP identity');
    } else {
      this.session = (await store.call('', 'join', input)).id;
      this.leave = true;
    }
    await this.own();
  }

  async init(value: unknown): Promise<void> {
    if (this.session) throw new ValidationError('This MCP connection is already initialized');
    const { input } = catalog.command('join', value);
    catalog.text(input, 'name');
    catalog.text(input, 'vendor');
    if (input.vendorSession !== undefined) catalog.text(input, 'vendorSession');
    const store = this.open(true);
    try {
      const binding = initialize(store, input);
      this.session = binding.session;
      this.leave = binding.created;
      if (binding.created) await this.own();
    } catch (error) {
      await this.close();
      throw error;
    }
  }

  private async own(): Promise<void> {
    const store = this.store!;
    this.owner = await claimDeliveryOwner(store.database, this.session);
    this.manual();
    await store.present(this.session);
    this.leave = true;
    this.managed = true;
  }

  private manual(): void {
    if (this.store!.db.value("SELECT EXISTS(SELECT 1 FROM attachments WHERE session=? AND transport!='raw')", [this.session])) {
      throw new ValidationError('Managed MCP owns manual inbox presence only; use plain mcp alongside the existing native delivery owner');
    }
  }

  async renew(): Promise<void> {
    this.manual();
    await this.store!.call(this.session, 'heartbeat', { renewLeases: true });
  }

  describe() {
    const store = this.store!;
    return { session: this.session, workspace: store.workspace, coordinationScope: store.coordinationScope,
      branch: store.known(this.session).branch ?? null, tools: catalog.selectedTools(this.args.tools).map(tool => tool.name) };
  }

  instructions(): string {
    return this.session
      ? mcpInstructions(this.describe(), this.managed, catalog.selectedTools(this.args.tools))
      : 'Call init with your name and vendor. Use vendorSession only if the host supplied its exact current ID; never copy a peer identity or infer one from hook files. Follow the returned tools and instructions.';
  }

  async close(): Promise<void> {
    try {
      if (this.leave && this.session) await this.store?.call(this.session, 'leave', {});
    } finally {
      try { await this.owner?.release(); }
      finally {
        this.store?.close();
        this.store = null;
        this.session = '';
        this.owner = null;
        this.leave = false;
        this.managed = false;
      }
    }
  }
}

export function initTool() {
  return { name: 'init', description: 'Create a managed identity or reuse an exact live vendorSession binding under its external lifecycle owner. Returns tools/instructions; disappears after success.',
    inputSchema: catalog.definition('join').inputSchema,
    annotations: { readOnlyHint: false, destructiveHint: false, openWorldHint: false } };
}
