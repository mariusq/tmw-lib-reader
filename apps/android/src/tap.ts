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
