export function maskEveryOtherChar(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    result += i % 2 === 0 ? '*' : text[i];
  }
  return result;
}
