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
