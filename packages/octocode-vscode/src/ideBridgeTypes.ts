export type Json =
  null | boolean | number | string | Json[] | { [key: string]: Json };

export type Obj = Record<string, unknown>;

export type Family = 'call' | 'type';

/** Stores a provider's hierarchy item and returns its opaque `data` handle. */
export type Remember = (family: Family, item: unknown) => string;
