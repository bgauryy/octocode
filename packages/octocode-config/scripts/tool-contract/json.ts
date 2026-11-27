// The JSON the tool-contract generator reads and writes, and its guards.
export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };

export const isObject = (value: Json | undefined): value is JsonObject =>
  !!value && typeof value === 'object' && !Array.isArray(value);

export const pascal = (value: string): string =>
  value.charAt(0).toUpperCase() + value.slice(1);
