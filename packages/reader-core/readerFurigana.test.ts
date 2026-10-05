import { expect, it } from "vitest";
import { applyReaderFurigana } from "./readerFurigana";

it("toggles ruby annotations without altering text or creating duplicate styles", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML = "<ruby>日本<rp>(</rp><rt>にほん</rt><rp>)</rp></ruby>";
  const original = doc.body.innerHTML;
  applyReaderFurigana(doc, true);
  expect(doc.querySelector("style")?.textContent).toContain("display: none !important");
  applyReaderFurigana(doc, false);
  expect(doc.querySelector("style")?.textContent).toBe("");
  expect(doc.querySelectorAll("style[data-tmw-furigana]")).toHaveLength(1);
  expect(doc.body.innerHTML).toBe(original);
});

it("preserves fixed-layout publication styling", () => {
  const doc = document.implementation.createHTMLDocument();
  applyReaderFurigana(doc, true, true);
  expect(doc.querySelector("style[data-tmw-furigana]")).toBeNull();
});
