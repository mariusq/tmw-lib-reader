// Local calendar dates deliberately avoid UTC slicing (including DST boundaries).
export function localDay(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
export function calendarDays(now: Date, count = 364): Date[] {
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Array.from(
    { length: count },
    (_, index) => new Date(end.getFullYear(), end.getMonth(), end.getDate() - count + index + 1),
  );
}
export function weekStart(now: Date): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - ((now.getDay() + 6) % 7));
}
export type ActivityInterval = {
  day: string;
  startMs: number;
  endMs: number;
  milliseconds: number;
};
export function readingIntervals(start: number, end: number, elapsed: number): ActivityInterval[] {
  // Delayed timers after sleep, large clock changes and invalid intervals count nothing.
  if (![start, end, elapsed].every(Number.isFinite) || elapsed <= 0 || elapsed > 15000 || end <= start || Math.abs(end - start - elapsed) > 1000)
    return [];
  // Date.now has integer precision while performance.now has fractional precision.
  // Persist integer boundaries and never credit more than their representable span.
  const from = Math.ceil(start), to = Math.floor(end);
  if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to) || from < 0 || to <= from || to - from > 15000) return [];
  const credited = Math.min(elapsed, to - from);
  const result: ActivityInterval[] = [];
  let cursor = from;
  while (cursor < to) {
    const date = new Date(cursor);
    const midnight = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime();
    const boundary = Math.min(to, midnight);
    result.push({
      day: localDay(date),
      startMs: cursor,
      endMs: boundary,
      milliseconds: Math.floor((credited * (boundary - cursor)) / (to - from)),
    });
    cursor = boundary;
  }
  return result.filter((interval) => interval.milliseconds > 0);
}
