import { expect, it } from "vitest";
import { prepareReaderDocument, readerFontSize } from "./readerPresentation";

it("converts vertical publication styles to horizontal without changing text or ruby", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.head.innerHTML =
    "<style>body {writing-mode:vertical-rl} p {text-align:right;letter-spacing:1em}</style>";
  doc.body.innerHTML = '<p style="writing-mode:vertical-rl"><ruby>猫<rt>ねこ</rt></ruby>。</p>';
  const before = doc.body.innerHTML;
  prepareReaderDocument(doc, 24);
  expect(doc.documentElement.style.writingMode).toBe("horizontal-tb");
  expect(doc.querySelector("[data-tmw-reader]")?.textContent).toContain("writing-mode:horizontal-tb !important");
  expect(doc.documentElement.style.direction).toBe("ltr");
  expect(doc.body.innerHTML).toBe(before);
  const css = doc.querySelector("[data-tmw-reader]")?.textContent;
  expect(css).toContain("text-align:left !important");
  expect(css).toContain("letter-spacing:normal !important");
  expect(css).toContain("font-size:24px");
  expect(css).toContain("ruby-position:over");
});
it("leaves fixed-layout pages intact", () => {
  const doc = document.implementation.createHTMLDocument();
  const before = doc.documentElement.outerHTML;
  prepareReaderDocument(doc, 24, true);
  expect(doc.documentElement.outerHTML).toBe(before);
});
it("selects Japanese fallback for EPUBs with missing or incorrect language", () => {
  const doc = new DOMParser().parseFromString(
    '<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="zh" lang="en"><head/><body lang="zh"><p style="font-family: SimSun"><ruby>骨直令<rt>ほね</rt></ruby></p></body></html>',
    "application/xhtml+xml",
  );
  const text = doc.body.textContent;
  prepareReaderDocument(doc, 20);
  for (const element of [doc.documentElement, doc.body]) {
    expect(element.getAttribute("lang")).toBe("ja");
    expect(element.getAttributeNS("http://www.w3.org/XML/1998/namespace", "lang")).toBe("ja");
  }
  const css = doc.querySelector("[data-tmw-reader]")?.textContent;
  expect(css).toContain('html, body, body * {-webkit-locale:"ja" !important;');
  expect(css).toContain('font-family:"Noto Serif CJK JP","Noto Serif JP",serif !important;');
  expect(doc.body.textContent).toBe(text);
});
it("bounds persisted font size", () => {
  expect(readerFontSize(null)).toBe(20);
  expect(readerFontSize("24")).toBe(24);
  expect(readerFontSize("Infinity")).toBe(20);
  expect(readerFontSize("100")).toBe(20);
});


it("supports vertical right-to-left and can switch back without changing ruby", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML = "<p><ruby>text<rt>reading</rt></ruby></p>";
  const before = doc.body.innerHTML;
  prepareReaderDocument(doc, 20, false, "vertical-rl");
  expect(doc.documentElement.style.writingMode).toBe("vertical-rl");
  expect(doc.documentElement.style.direction).toBe("ltr");
  prepareReaderDocument(doc, 20, false, "horizontal-tb");
  expect(doc.documentElement.style.writingMode).toBe("horizontal-tb");
  expect(doc.querySelectorAll("[data-tmw-reader]")).toHaveLength(1);
  expect(doc.body.innerHTML).toBe(before);
});
