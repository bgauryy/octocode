interface UiEvent {
  kind: string;
  data?: unknown;
}

export function semanticUi(
  events: UiEvent[],
  observations: UiEvent[] = [],
): Record<string, unknown> {
  const push = (kind: string, data?: unknown): void => {
    events.push({ kind, ...(data === undefined ? {} : { data }) });
  };
  return {
    select: async (title: string, options: string[]) => {
      push("ui.select", { title, count: options.length });
      return options[0];
    },
    confirm: async (title: string) => {
      push("ui.confirm", { title });
      return true;
    },
    input: async (title: string) => {
      push("ui.input", { title });
      return "probe-input";
    },
    editor: async (title: string) => {
      push("ui.editor", { title });
      return "probe-editor";
    },
    notify: (message: string, type?: string) => {
      if (message.startsWith("probe:")) push("ui.notify", { message, type });
      else
        observations.push({
          kind: "ui.host-notification",
          data: { message, type },
        });
    },
    onTerminalInput: () => () => undefined,
    setStatus: (key: string, value?: string) => {
      if (key.startsWith("probe"))
        push("ui.status", { key, active: value !== undefined });
    },
    setWorkingMessage: () => undefined,
    setWorkingVisible: () => undefined,
    setWorkingIndicator: () => undefined,
    setHiddenThinkingLabel: () => undefined,
    setWidget: () => undefined,
    setFooter: () => undefined,
    setHeader: () => undefined,
    setTitle: (title: string) => {
      if (title.startsWith("probe")) push("ui.title", { title });
    },
    custom: async () => undefined,
    pasteToEditor: () => undefined,
    setEditorText: () => undefined,
    getEditorText: () => "",
    addAutocompleteProvider: () => undefined,
    setEditorComponent: () => undefined,
    getEditorComponent: () => undefined,
    // This probe records UI semantics, not ANSI rendering. Supply the public
    // theme operations used by the product instead of an invalid empty theme.
    theme: {
      fg: (_color: string, value: string) => value,
      bg: (_color: string, value: string) => value,
      bold: (value: string) => value,
      italic: (value: string) => value,
      underline: (value: string) => value,
    },
    getAllThemes: () => [],
    getTheme: () => undefined,
    setTheme: () => ({ success: false }),
    getToolsExpanded: () => false,
    setToolsExpanded: () => undefined,
  };
}
