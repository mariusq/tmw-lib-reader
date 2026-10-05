import { describe, expect, it } from "vitest";
import { edgePageTurn, isLookupTap } from "./tap";
describe("reader tap disambiguation", () => {
  const start = { x: 10, y: 20, time: 0, id: 1 };
  it("accepts a short stationary tap", () =>
    expect(isLookupTap(start, { ...start, time: 100 }, false)).toBe(true));
  it("rejects swipes, scrolling, long presses and different pointers", () => {
    expect(isLookupTap(start, { ...start, x: 40, time: 100 }, false)).toBe(false);
    expect(isLookupTap(start, { ...start, time: 100 }, true)).toBe(false);
    expect(isLookupTap(start, { ...start, time: 500 }, false)).toBe(false);
    expect(isLookupTap(start, { ...start, id: 2, time: 100 }, false)).toBe(false);
    expect(isLookupTap(undefined, start, false)).toBe(false);
  });
});

it("uses visible-pane edge bands and rejects interior, outside and cross-band taps", () => {
  expect(edgePageTurn(380, 378, 390)).toBe("next");
  expect(edgePageTurn(10, 12, 390)).toBe("previous");
  expect(edgePageTurn(190, 190, 390)).toBeUndefined();
  expect(edgePageTurn(-1, 0, 390)).toBeUndefined();
  expect(edgePageTurn(395, 395, 390)).toBeUndefined();
  expect(edgePageTurn(350, 360, 390)).toBeUndefined();
  expect(edgePageTurn(0, 0, 0)).toBeUndefined();
});
