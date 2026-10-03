import { describe, expect, it } from "vitest";
import { sentenceAt } from "./passageContext";
import { dictionaryTextAt } from "./dictionaryText";

describe("passage context", () => {
  it("extracts the clicked Japanese sentence including punctuation", () => {
    expect(sentenceAt("前の文。溜息を吐く！次の文。", 5)).toBe("溜息を吐く！");
    expect(sentenceAt("😀。漢字です。", 3)).toBe("漢字です。");
  });
  it("handles missing boundaries and empty text", () => {
    expect(sentenceAt("境界なし", 1)).toBe("境界なし");
    expect(sentenceAt("", 0)).toBe("");
    expect(sentenceAt("a".repeat(5000), 0).length).toBe(4000);
  });
  it("uses ruby-free context even in vertical text", () => {
    document.body.innerHTML = '<p style="writing-mode:vertical-rl">前。<ruby>漢字<rt>かんじ</rt></ruby>を読む。</p>';
    const root = document.querySelector("p")!;
    const node = document.querySelector("ruby")!.firstChild as Text;
    const target = dictionaryTextAt(root, node, 0)!;
    expect(sentenceAt(target.text, target.offset)).toBe("漢字を読む。");
  });
});
