// Text assets that src/build.ts inlines with esbuild's `text` loader.
declare module '*.sql' {
  const text: string;
  export default text;
}
declare module '*.md' {
  const text: string;
  export default text;
}
declare module '*.html' {
  const text: string;
  export default text;
}
declare module '*.css' {
  const text: string;
  export default text;
}
declare module '*/dashboard/app.js' {
  const text: string;
  export default text;
}
