export const READER_THEME_KEY = "tmw-reader-theme";
export const readerThemes = {
  light: { label: "Light", background: "#f5f5f4", foreground: "#1c1917", link: "#754b17", dark: false },
  paper: { label: "Warm paper", background: "#faf7ef", foreground: "#292524", link: "#754b17", dark: false },
  sepia: { label: "Sepia", background: "#e8dcc5", foreground: "#3d3024", link: "#754b17", dark: false },
  // Trans flag pastels: pink-white page, deep blue text, flag-blue links.
  pastel: { label: "Pastel", background: "#fdf0f5", foreground: "#2b3352", link: "#2873a8", dark: false },
  matcha: { label: "Matcha", background: "#eef2e3", foreground: "#28301f", link: "#4a6a26", dark: false },
  dusk: { label: "Dusk", background: "#35312e", foreground: "#e5dacc", link: "#f0c98b", dark: true },
  pastelNight: { label: "Pastel night", background: "#272334", foreground: "#f4dde6", link: "#8fd3f4", dark: true },
  midnight: { label: "Midnight", background: "#131a2b", foreground: "#d7deeb", link: "#93b7ff", dark: true },
  dark: { label: "Dark", background: "#0c0a09", foreground: "#f5f5f4", link: "#f0c98b", dark: true },
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
  style.textContent = `html, body {background:${palette.background} !important; color:${palette.foreground} !important;} html {color-scheme:${palette.dark ? "dark" : "light"};} body :where(p, div, span, li, blockquote, h1, h2, h3, h4, h5, h6, ruby, rt) {color:inherit !important;} a {color:${palette.link} !important;}`;
}
