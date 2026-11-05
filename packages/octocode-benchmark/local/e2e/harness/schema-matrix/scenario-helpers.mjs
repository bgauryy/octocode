export const rows = json => json?.results ?? [];
export const dataOf = result => result.json?.results?.[0]?.data ?? {};
export const errorOf = result => result.json?.errorCode ?? dataOf(result).errorCode;
