/** Browser assets are inlined as text by Vite (tests) and build.mjs (bundle). */
declare module '*?raw' {
  const content: string;
  export default content;
}
