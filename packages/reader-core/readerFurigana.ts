export const READER_FURIGANA_KEY = "tmw-reader-hide-furigana";

// Hide annotations through CSS only so base text, lookup offsets and CFIs survive.
export function applyReaderFurigana(doc: Document, hidden: boolean, fixedLayout = false) {
  if (fixedLayout) return;
  let style = doc.querySelector<HTMLStyleElement>("style[data-tmw-furigana]");
  if (!style) {
    style = doc.createElement("style");
    style.setAttribute("data-tmw-furigana", "");
    (doc.head ?? doc.documentElement).append(style);
  }
  style.textContent = hidden ? "rt, rp { display: none !important; }" : "";
}
