import { expect, it } from "vitest";
import { applyReaderColors, readerTheme, readerThemes } from "./readerTheme";

it("keeps existing preferences and safely defaults unknown stored values", () => {
  expect(readerTheme("light", "dark")).toBe("light");
  expect(readerTheme("dark", "paper")).toBe("dark");
  expect(readerTheme("paper", "dark")).toBe("paper");
  expect(readerTheme("toString", "paper")).toBe("paper");
  expect(readerTheme(null, "paper")).toBe("paper");
});
it("switches colors in place without touching book text or accumulating styles", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML = '<p><ruby>日本<rt>にほん</rt></ruby></p>';
  const original = doc.body.innerHTML;
  applyReaderColors(doc, "paper");
  expect(doc.querySelector("[data-tmw-colors]")?.textContent).toContain(readerThemes.paper.background);
  applyReaderColors(doc, "dusk");
  expect(doc.querySelectorAll("[data-tmw-colors]")).toHaveLength(1);
  expect(doc.querySelector("[data-tmw-colors]")?.textContent).toContain("color-scheme:dark");
  expect(doc.body.innerHTML).toBe(original);
});
it("preserves fixed-layout book styling", () => {
  const doc = document.implementation.createHTMLDocument();
  applyReaderColors(doc, "dark", true);
  expect(doc.querySelector("[data-tmw-colors]")).toBeNull();
});
