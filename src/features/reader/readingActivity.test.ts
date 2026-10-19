import { describe, expect, it } from "vitest";
import {
  calendarDays,
  localDay,
  readingIntervals,
  weekStart,
} from "../../../packages/reader-core/readingActivity";
describe("reading activity calendar", () => {
  it("uses local dates and Monday weeks across years", () => {
    const now = new Date(2026, 0, 1, 1);
    expect(localDay(now)).toBe("2026-01-01");
    expect(localDay(weekStart(now))).toBe("2025-12-29");
    const days = calendarDays(now);
    expect(days).toHaveLength(364);
    expect(localDay(days[days.length - 1])).toBe("2026-01-01");
    expect(new Set(days.map(localDay)).size).toBe(364);
  });
  it("splits a midnight interval without losing or doubling time", () => {
    const start = new Date(2026, 9, 13, 23, 59, 55).getTime();
    expect(readingIntervals(start, start + 10000, 10000)).toEqual([
      { day: "2026-10-13", startMs: start, endMs: start + 5000, milliseconds: 5000 },
      { day: "2026-10-14", startMs: start + 5000, endMs: start + 10000, milliseconds: 5000 },
    ]);
  });
  it("rejects sleep delays, clock jumps and negative intervals", () => {
    expect(readingIntervals(10000, 70000, 60000)).toEqual([]);
    expect(readingIntervals(10000, 70000, 10000)).toEqual([]);
    expect(readingIntervals(10000, 9000, 10000)).toEqual([]);
  });
  it("normalizes fractional monotonic drift to native and sync-safe integers", () => {
    for (const [start,end,elapsed] of [[10000,19999,10000.2],[10000,20000.75,10000.75],[10000.75,20000.5,9999.75]]) {
      const rows=readingIntervals(start,end,elapsed);
      expect(rows.length).toBeGreaterThan(0);
      for (const row of rows) {
        expect(Number.isSafeInteger(row.startMs)).toBe(true);
        expect(Number.isSafeInteger(row.endMs)).toBe(true);
        expect(row.milliseconds).toBeLessThanOrEqual(row.endMs-row.startMs);
      }
    }
    expect(readingIntervals(10000,19999,10000.2)[0].milliseconds).toBe(9999);
    expect(readingIntervals(10000,25001,14999.9)).toEqual([]);
    expect(readingIntervals(10000,20000,NaN)).toEqual([]);
  });
});
