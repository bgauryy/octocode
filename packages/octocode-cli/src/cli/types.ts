export interface ParsedArgs {
  command: string | null;
  args: string[];
  options: Record<string, string | boolean>;
}

interface CLIOption {
  name: string;
  description: string;
  hasValue?: boolean;
  default?: string | boolean;
}

export interface CLICommandSpec {
  name: string;
  description: string;
  usage?: string;
  scheme?: string[];
  whenToUse?: string[];
  examples?: string[];
  options?: CLIOption[];
}

export interface CLICommand extends CLICommandSpec {
  handler: (args: ParsedArgs) => Promise<void> | void;
}
