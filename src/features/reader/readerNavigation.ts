type PageRendition = { next: () => Promise<unknown>; prev: () => Promise<unknown> };
type Position = { index: number; cfi: string; displayed: { page: number; total: number } };
export type PageLocation = { start?: Position; end?: Position };
type Recovery = {
  location: () => PageLocation | undefined;
  nextSection: (index: number) => Promise<unknown>;
  settle?: () => Promise<void>;
};
const turning = new WeakSet<PageRendition>();

async function settlePage() {
  // reportLocation schedules its relocation event on the next animation frame.
  // Wait for that frame and the browser's layout before examining live pagination.
  await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
}

function samePosition(before: PageLocation, after: PageLocation) {
  return before.start?.cfi === after.start?.cfi && before.end?.cfi === after.end?.cfi
    && before.start?.index === after.start?.index && before.end?.index === after.end?.index
    && before.start?.displayed.page === after.start?.displayed.page
    && before.end?.displayed.page === after.end?.displayed.page;
}

export async function turnReaderPage(view: PageRendition, direction: "next" | "previous", recovery?: Recovery) {
  if (turning.has(view)) return;
  turning.add(view);
  try {
    const before = recovery?.location();
    if (direction === "next") await view.next();
    else await view.prev();
    if (!recovery || direction !== "next" || !before?.start || !before.end) return;
    await (recovery.settle ?? settlePage)();
    const after = recovery.location();
    if (!after?.start || !after.end || !samePosition(before, after)) return;
    // RTL scrolling can clamp at a spine boundary without switching sections.
    // Recover only at a confirmed last page, never from an interior no-op.
    const { page, total } = after.end.displayed;
    if (Number.isFinite(page) && Number.isFinite(total) && total > 0 && page >= total) {
      await recovery.nextSection(after.end.index);
    }
  } finally {
    turning.delete(view);
  }
}
