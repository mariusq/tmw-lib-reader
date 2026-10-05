import { applyReaderWriting, type ReaderWriting } from "../../../packages/reader-core/readerWriting";
export const FONT_SIZE_KEY = "tmw-reader-font-size";
export function readerFontSize(value: string | null): number {
  const size = Number(value);
  return Number.isFinite(size) && size >= 16 && size <= 32 ? Math.round(size) : 20;
}

// Apply before serialization so epub.js measures the horizontal pagination axis.
// This changes only the in-memory presentation; text, ruby and CFIs stay intact.
export function prepareReaderDocument(doc: Document, fontSize: number, fixedLayout = false, mode: ReaderWriting = "horizontal-tb") {
  const html = doc.documentElement;
  const body = doc.body ?? doc.querySelector("body");
  if (!body || fixedLayout) return;
  html.style.setProperty("writing-mode", mode, "important");
  html.style.setProperty("direction", "ltr", "important");
  const style = doc.createElement("style");
  style.setAttribute("data-tmw-reader", "");
  style.textContent = `html {color-scheme:light; background:#faf7ef !important;}
    html, body, body * {writing-mode:${mode} !important;
      -epub-writing-mode:${mode} !important; -webkit-writing-mode:${mode} !important;
      direction:ltr !important; text-orientation:mixed !important;}
    body {color:#292524 !important; background:#faf7ef !important;
      font-family:"Noto Serif CJK JP","Noto Serif JP",serif !important;
      font-size:${fontSize}px !important;}
    body, p, div, li, blockquote, h1, h2, h3, h4, h5, h6, span {
      text-align:left !important; text-indent:0 !important;
      letter-spacing:normal !important; word-spacing:normal !important;
      white-space:normal !important; line-height:1.8 !important;
      overflow-wrap:break-word; word-break:normal;}
    p {margin-block:0 1em !important;}
    ruby {ruby-position:over;} rt {font-size:0.55em !important; line-height:1 !important;}
    img,svg {max-width:100%; max-height:100%;} a {color:#92400e;}`;
  doc.querySelector("style[data-tmw-reader]")?.remove();
  (doc.head ?? html).append(style);
  applyReaderWriting(doc, mode);
}
