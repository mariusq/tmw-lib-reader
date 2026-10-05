import { expect, it } from "vitest";
import { readerProgress } from "./readerProgress";

it("uses whole-book character positions across chapter boundaries", () => {
  const positions: Record<string, number> = { front: 0.007, chapter: 0.008, middle: 0.5 };
  const locations = { percentageFromCfi: (cfi: string) => positions[cfi] ?? null };
  expect(readerProgress({ start: { cfi: "front" } }, locations)).toBeCloseTo(0.7);
  expect(readerProgress({ start: { cfi: "chapter" } }, locations)).toBeCloseTo(0.8);
  expect(readerProgress({ start: { cfi: "middle" } }, locations)).toBe(50);
});
it("hides unavailable progress and bounds percentages", () => {
  expect(readerProgress({}, { percentageFromCfi: () => 0 })).toBeNull();
  for (const fraction of [null, NaN, Infinity]) {
    expect(readerProgress({ start: { cfi: "position" } }, { percentageFromCfi: () => fraction })).toBeNull();
  }
  expect(readerProgress({ start: { cfi: "position" } }, { percentageFromCfi: () => 1.1 })).toBe(100);
});
