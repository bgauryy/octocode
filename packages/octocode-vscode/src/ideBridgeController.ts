import * as vscode from 'vscode';

import { errorMessage } from './errors';
import { IdeBridge, type IdeBridgeOptions } from './ideBridge';

export const IDE_BRIDGE_SETTING = 'ideBridge.enabled';

/** Starts and stops one bridge as `octocode.ideBridge.enabled` changes. */
export class IdeBridgeController implements vscode.Disposable {
  private readonly bridge: IdeBridge;
  private queue: Promise<void> = Promise.resolve();
  private listener: vscode.Disposable | undefined;

  constructor(private readonly options: IdeBridgeOptions) {
    this.bridge = new IdeBridge(options);
  }

  /** Applies the setting now and on every change. */
  activate(): Promise<void> {
    this.listener = vscode.workspace.onDidChangeConfiguration(event => {
      if (event.affectsConfiguration(`octocode.${IDE_BRIDGE_SETTING}`)) {
        void this.sync();
      }
    });
    return this.sync();
  }

  sync(): Promise<void> {
    const apply = async () => {
      const enabled = vscode.workspace
        .getConfiguration('octocode')
        .get<boolean>(IDE_BRIDGE_SETTING, true);
      try {
        if (enabled) await this.bridge.start();
        else await this.bridge.stop();
      } catch (error) {
        this.options.output(`IDE bridge failed: ${errorMessage(error)}`);
      }
    };
    this.queue = this.queue.then(apply);
    return this.queue;
  }

  async stop(): Promise<void> {
    this.listener?.dispose();
    this.listener = undefined;
    await this.queue;
    await this.bridge.stop();
  }

  dispose(): void {
    void this.stop();
  }
}
