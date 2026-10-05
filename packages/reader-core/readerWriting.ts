export const READER_WRITING_KEY = "tmw-reader-writing-mode";
export type ReaderWriting = "horizontal-tb" | "vertical-rl";
export function readerWriting(value: string | null): ReaderWriting {
  return value === "vertical-rl" || value === "vertical-lr" ? "vertical-rl" : "horizontal-tb";
}
export function applyReaderWriting(doc: Document, mode: ReaderWriting, fixed = false) {
  if (fixed) return;
  doc.documentElement.style.setProperty("writing-mode", mode, "important");
  doc.documentElement.style.setProperty("direction", "ltr", "important");
  let style = doc.querySelector<HTMLStyleElement>("style[data-tmw-writing]");
  if (!style) {
    style = doc.createElement("style");
    style.setAttribute("data-tmw-writing", "");
    (doc.head ?? doc.documentElement).append(style);
  }
  style.textContent = `html, body, body * {writing-mode:${mode} !important; -epub-writing-mode:${mode} !important; -webkit-writing-mode:${mode} !important; direction:ltr !important; text-orientation:mixed !important;}
    img {max-width:100% !important; max-height:90vh !important; width:auto !important; height:auto !important; object-fit:contain; display:block; margin:0 auto;}
    svg {max-width:100% !important; max-height:90vh !important;}`;
}
