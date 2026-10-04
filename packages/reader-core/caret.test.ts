import { describe, expect, it, vi } from "vitest";
import { caretAt } from "./caret";
describe("glyph hit detection", () => {
  it("selects the glyph before an insertion boundary in either writing mode", () => {
    const node = document.createTextNode("😀猫");
    const range = document.createRange();
    range.setStart(node, 3);
    Object.defineProperty(document, "caretRangeFromPoint", {
      configurable: true,
      value: () => range,
    });
    const original = Object.getOwnPropertyDescriptor(Range.prototype, "getClientRects");
    Object.defineProperty(Range.prototype, "getClientRects", {
      configurable: true,
      value: vi.fn(function (this: Range) {
        return (this.startOffset === 2
          ? [{ left: 10, right: 30, top: 40, bottom: 60 }]
          : []) as unknown as DOMRectList;
      }),
    });
    expect(caretAt(document, 25, 55)).toEqual({ node, offset: 2 });
    expect(caretAt(document, 100, 100)).toBeNull();
    if (original) Object.defineProperty(Range.prototype, "getClientRects", original);
    else Reflect.deleteProperty(Range.prototype, "getClientRects");
    delete (document as Document & { caretRangeFromPoint?: unknown }).caretRangeFromPoint;
  });
});
