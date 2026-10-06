export interface RuntimeCLIOption {
  name: string;
  hasValue?: boolean;
  default?: string | number | boolean;
}

export interface ParsedArgs {
  command: string | null;
  args: string[];
  options: Record<string, string | boolean>;
  /** The unmodified argv the parser consumed (order- and repeat-preserving). */
  raw?: string[];
}

// A runnable Node-owned command: its name, the options it validates, and a
// handler. Help text lives with the handler.
export interface CLICommand {
  name: string;
  options?: RuntimeCLIOption[];
  handler: (args: ParsedArgs) => Promise<void> | void;
}
