interface JSON {
  rawJSON(text: string): unknown;
  isRawJSON(value: unknown): boolean;
  parse(
    text: string,
    reviver: (key: string, value: any, context?: { source: string }) => any
  ): any;
}
