export async function fetchUser(id: number) {
  const res = await fetch(`/api/users/${id}`);
  if (!res.ok) throw new Error("failed");
  return res.json();
}

export function formatName(first: string, last: string) {
  return `${first} ${last}`.trim();
}
