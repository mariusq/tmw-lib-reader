export type ProgressLocation = { start?: { cfi?: string } };
export type CharacterLocations = { percentageFromCfi: (cfi: string) => number | null };

// Use character positions across the whole reading order, independent of pagination.
export function readerProgress(location: ProgressLocation, locations: CharacterLocations): number | null {
  const cfi = location.start?.cfi;
  if (!cfi) return null;
  const fraction = locations.percentageFromCfi(cfi);
  if (fraction === null || !Number.isFinite(fraction)) return null;
  return Math.max(0, Math.min(100, fraction * 100));
}
