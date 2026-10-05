export type TouchStart = { x: number; y: number; time: number; id: number };
export function isLookupTap(
  start: TouchStart | undefined,
  end: TouchStart,
  moved: boolean,
): boolean {
  return (
    !!start &&
    !moved &&
    start.id === end.id &&
    end.time - start.time <= 450 &&
    Math.hypot(end.x - start.x, end.y - start.y) <= 10
  );
}

// Only blank-margin taps reach this helper. Text glyphs always retain lookup.
// Require both ends within the same 32px edge band of the visible pane.
export function edgePageTurn(
  startX: number,
  endX: number,
  width: number,
): "next" | "previous" | undefined {
  if (!Number.isFinite(width) || width <= 0) return;
  const band = Math.min(32, width / 4);
  if (startX >= 0 && startX < band && endX >= 0 && endX < band) return "previous";
  if (startX > width - band && startX <= width && endX > width - band && endX <= width)
    return "next";
}
