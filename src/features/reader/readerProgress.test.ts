import { describe, expect, it } from "vitest";
import { readerProgress } from "./readerProgress";

describe("lightweight reader progress", () => {
  const start = { index: 3, displayed: { page: 6, total: 10 } };
  it("uses reading order and the current chapter page, excluding non-linear sections", () => {
    expect(readerProgress({ start }, [1, 3, 4, 5])).toBe(37.5);
  });
  it("reports the publication boundaries including a one-page book", () => {
    expect(readerProgress({ start, atStart: true }, [3])).toBe(0);
    expect(readerProgress({ start, atEnd: true }, [3])).toBe(100);
  });
  it("hides progress when the position is unavailable or outside the reading order", () => {
    expect(readerProgress({}, [3])).toBeNull();
    expect(readerProgress({ start }, [])).toBeNull();
    expect(readerProgress({ start }, [1])).toBeNull();
    expect(readerProgress({ start: { ...start, displayed: { page: 1, total: 0 } } }, [3])).toBeNull();
  });
});
