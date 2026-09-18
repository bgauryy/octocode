# Q8 — VS Code keybinding dispatch

The concrete workbench service is `WorkbenchKeybindingService` in [`src/vs/workbench/services/keybinding/browser/keybindingService.ts`](https://github.com/microsoft/vscode/blob/265cdf226b54b8cbe19f37e748ba229fd7714b97/src/vs/workbench/services/keybinding/browser/keybindingService.ts#L175-L200). It extends `AbstractKeybindingService`, defined in [`src/vs/platform/keybinding/common/abstractKeybindingService.ts`](https://github.com/microsoft/vscode/blob/265cdf226b54b8cbe19f37e748ba229fd7714b97/src/vs/platform/keybinding/common/abstractKeybindingService.ts#L42-L62).

The public entry receiving the keyboard event is `dispatchEvent(e: IKeyboardEvent, target: IContextKeyServiceTarget): boolean`; it delegates to `_dispatch(e, target)` ([lines 143–145](https://github.com/microsoft/vscode/blob/265cdf226b54b8cbe19f37e748ba229fd7714b97/src/vs/platform/keybinding/common/abstractKeybindingService.ts#L143-L145)).
