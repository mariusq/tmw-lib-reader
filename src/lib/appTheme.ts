export const APP_THEME_KEY = "tmw-app-theme";
// Swatches mirror the CSS tokens in styles.css (page, panel, accent) for the picker preview.
export const appThemes = {
  dark: { ja: "ダーク", en: "Dark", swatches: ["#0c0a09", "#292524", "#fbbf24"] },
  pastel: { ja: "パステル", en: "Pastel", swatches: ["#fdf3f7", "#5bcefa", "#f5a9b8"] },
  pastelNight: { ja: "パステルナイト", en: "Pastel night", swatches: ["#1c1928", "#5bcefa", "#f5a9b8"] },
  matcha: { ja: "抹茶", en: "Matcha", swatches: ["#f1f4e8", "#dbe2c9", "#4f7226"] },
  midnight: { ja: "ミッドナイト", en: "Midnight", swatches: ["#0d1220", "#1f2840", "#7aa7ff"] },
} as const;
export type AppTheme = keyof typeof appThemes;

export function appTheme(value: string | null): AppTheme {
  return value && Object.prototype.hasOwnProperty.call(appThemes, value) ? value as AppTheme : "dark";
}

export function storedAppTheme(): AppTheme {
  try { return appTheme(localStorage.getItem(APP_THEME_KEY)); } catch { return "dark"; }
}

export function applyAppTheme(theme: AppTheme, root: HTMLElement = document.documentElement) {
  if (theme === "dark") delete root.dataset.appTheme;
  else root.dataset.appTheme = theme;
}

export function saveAppTheme(theme: AppTheme) {
  applyAppTheme(theme);
  try { localStorage.setItem(APP_THEME_KEY, theme); } catch { /* preference is a convenience */ }
}
