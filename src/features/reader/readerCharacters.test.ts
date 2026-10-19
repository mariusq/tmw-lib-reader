import { expect, it } from "vitest";
import {
  captureCharacterPage,
  readingCharacterCount,
} from "../../../packages/reader-core/readerCharacters";
import type { PageLocation } from "../../../packages/reader-core/readerNavigation";
function location(start = "from", end = "to", index = 0, endIndex = index): PageLocation {
  return {
    start: { cfi: start, index, displayed: { page: 1, total: 2 } },
    end: { cfi: end, index: endIndex, displayed: { page: 1, total: 2 } },
  };
}
it("counts Unicode scalars without whitespace or ruby annotations", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML =
    "<p>前 <ruby>本<rt>ほん</rt><rp>(</rp></ruby> を𠮷読む。</p><span hidden>除外</span><script>bad</script>";
  const from = doc.createRange(),
    to = doc.createRange();
  const p = doc.querySelector("p")!;
  from.setStart(p.firstChild!, 1);
  from.collapse(true);
  to.setStart(p.lastChild!, p.lastChild!.textContent!.length);
  to.collapse(true);
  const page = captureCharacterPage({
    currentLocation: () => location(),
    getRange: (cfi) => (cfi === "from" ? from : to),
  });
  expect(page).toEqual({ spine: 0, startOffset: 1, endOffset: 7, cfi: "from" });
  expect(readingCharacterCount("𠮷 読\nむ。")).toBe(4);
});
it("resolves element boundaries and excludes image-only/unsafe ranges", () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML = "<p>本<ruby>語<rt>ご</rt></ruby></p><p>次</p>";
  const from = doc.createRange(),
    to = doc.createRange();
  from.setStart(doc.body, 0);
  from.collapse(true);
  to.setStart(doc.body, 1);
  to.collapse(true);
  expect(
    captureCharacterPage({
      currentLocation: () => location(),
      getRange: (cfi) => (cfi === "from" ? from : to),
    }),
  ).toEqual({ spine: 0, startOffset: 0, endOffset: 2, cfi: "from" });
  expect(
    captureCharacterPage({
      currentLocation: () => location("from", "to", 0, 1),
      getRange: () => from,
    }),
  ).toBeNull();
  expect(
    captureCharacterPage({ currentLocation: () => location(), getRange: () => from }),
  ).toBeNull();
  expect(captureCharacterPage({ currentLocation: () => undefined })).toBeNull();
});
it("excludes text hidden by inherited publication CSS", () => {
  const host=document.createElement("div");
  host.innerHTML='<p>本<span style="display:none">隠す</span><span style="visibility:hidden">消す</span>語</p>';
  document.body.append(host);
  const from=document.createRange(),to=document.createRange();
  from.setStart(host,0);from.collapse(true);to.setStart(host,1);to.collapse(true);
  expect(captureCharacterPage({currentLocation:()=>location(),getRange:cfi=>cfi==="from"?from:to})?.endOffset).toBe(2);
  host.remove();
});
