export const READER_THEME_KEY = "tmw-reader-theme";
export const readerThemes = {
  light: { label: "Light", background: "#f5f5f4", foreground: "#1c1917", dark: false },
  paper: { label: "Warm paper", background: "#faf7ef", foreground: "#292524", dark: false },
  sepia: { label: "Sepia", background: "#e8dcc5", foreground: "#3d3024", dark: false },
  dusk: { label: "Dusk", background: "#35312e", foreground: "#e5dacc", dark: true },
  dark: { label: "Dark", background: "#0c0a09", foreground: "#f5f5f4", dark: true },
} as const;
export type ReaderTheme = keyof typeof readerThemes;
export function readerTheme(value: string | null, fallback: ReaderTheme): ReaderTheme {
  return value && Object.prototype.hasOwnProperty.call(readerThemes, value) ? value as ReaderTheme : fallback;
}
export function applyReaderColors(doc: Document, theme: ReaderTheme, fixedLayout = false) {
  if (fixedLayout) return;
  let style = doc.querySelector<HTMLStyleElement>("style[data-tmw-colors]");
  if (!style) {
    style = doc.createElement("style");
    style.setAttribute("data-tmw-colors", "");
    (doc.head ?? doc.documentElement).append(style);
  }
  const palette = readerThemes[theme];
  style.textContent = `html, body {background:${palette.background} !important; color:${palette.foreground} !important;} html {color-scheme:${palette.dark ? "dark" : "light"};} body :where(p, div, span, li, blockquote, h1, h2, h3, h4, h5, h6, ruby, rt) {color:inherit !important;} a {color:${palette.dark ? "#f0c98b" : "#754b17"} !important;}`;
}
