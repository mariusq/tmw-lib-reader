export type ProgressLocation = {
  start?: { index: number; displayed: { page: number; total: number } };
  atStart?: boolean;
  atEnd?: boolean;
};

// Equal weight per reading-order section avoids loading off-screen chapters.
// This is an estimate: chapters can differ considerably in length.
export function readerProgress(location: ProgressLocation, sectionIndices: number[]): number | null {
  const start = location.start;
  if (!start || sectionIndices.length === 0) return null;
  const section = sectionIndices.indexOf(start.index);
  if (section < 0) return null;
  if (location.atEnd) return 100;
  if (location.atStart) return 0;
  const { page, total } = start.displayed;
  if (!Number.isFinite(page) || !Number.isFinite(total) || total <= 0) return null;
  const fraction = Math.max(0, Math.min(1, (page - 1) / total));
  return Math.min(99.9, Math.max(0, 100 * (section + fraction) / sectionIndices.length));
}
