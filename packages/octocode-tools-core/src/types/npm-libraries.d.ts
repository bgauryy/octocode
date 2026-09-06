declare module '@npmcli/config' {
  export default class Config {
    constructor(options: {
      npmPath: string;
      definitions: Record<string, unknown>;
      shorthands: Record<string, unknown>;
      flatten: (
        input: Record<string, unknown>,
        output: Record<string, unknown>
      ) => void;
      argv: string[];
      env: NodeJS.ProcessEnv;
      cwd: string;
    });
    load(): Promise<void>;
    validate(): boolean;
    get(key: string): unknown;
    readonly flat: Record<string, unknown>;
  }
}

declare module '@npmcli/config/lib/definitions/index.js' {
  const config: {
    definitions: Record<string, unknown>;
    shorthands: Record<string, unknown>;
    flatten: (input: Record<string, unknown>, output: Record<string, unknown>) => void;
  };
  export default config;
}

declare module 'npm-registry-fetch' {
  const fetch: {
    json(url: string, options: Record<string, unknown>): Promise<unknown>;
  };
  export default fetch;
}
