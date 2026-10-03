import { describe, expect, it } from "vitest";
import { dictionaryTextAt, japaneseWordAt } from "./dictionaryText";

describe("dictionaryTextAt", () => {
  it("omits furigana while retaining the base text and click offset", () => {
    document.body.innerHTML = `<p>小さく、<ruby>溜<rt>た</rt>息<rt>めいき</rt></ruby>を吐く。</p>`;
    const root = document.querySelector("p")!;
    const target = document.querySelector("ruby")!.firstChild as Text;

    expect(dictionaryTextAt(root, target, 1)).toEqual({
      text: "小さく、溜息を吐く。",
      offset: 5,
    });
  });

  it("keeps offsets correct when identical text nodes occur earlier", () => {
    document.body.innerHTML = `<p><span>溜息</span>と<span>溜息</span></p>`;
    const root = document.querySelector("p")!;
    const target = document.querySelectorAll("span")[1].firstChild as Text;

    expect(dictionaryTextAt(root, target, 0)).toEqual({ text: "溜息と溜息", offset: 3 });
  });
});

describe("japaneseWordAt", () => {
  it("uses the Japanese run at the clicked offset instead of the first run", () => {
    expect(japaneseWordAt("前の語。溜息を吐く", 4)).toBe("溜息を吐く");
  });
});
