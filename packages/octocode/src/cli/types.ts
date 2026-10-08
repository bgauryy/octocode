export interface ParsedArgs {
  command: string | null;
  args: string[];
  options: Record<string, string | boolean>;
}
