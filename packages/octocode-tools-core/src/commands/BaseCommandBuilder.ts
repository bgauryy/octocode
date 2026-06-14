export abstract class BaseCommandBuilder {
  protected command: string;
  protected args: string[] = [];

  constructor(command: string) {
    this.command = command;
  }

  addFlag(flag: string): this {
    this.args.push(flag);
    return this;
  }

  protected addOption(option: string, value: string | number): this {
    this.args.push(option, String(value));
    return this;
  }

  protected addArg(arg: string): this {
    this.args.push(arg);
    return this;
  }

  build(): { command: string; args: string[] } {
    return {
      command: this.command,
      args: [...this.args],
    };
  }

  reset(): this {
    this.args = [];
    return this;
  }

  getArgs(): string[] {
    return [...this.args];
  }
}
