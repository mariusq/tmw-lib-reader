export type SeriesBook = { id: number; effectiveTitle: string; effectiveCreator: string; effectiveVolume: string; effectiveCoverPath: string | null; isAvailable: boolean; isFinished: boolean };

// A volume number alone cannot establish copy identity. Keep bonus stories,
// revised editions and differing author credits separate.
export function groupSeriesCopies(books: SeriesBook[]) {
  const normalize = (value: string) => (value || "").normalize("NFKC").toLocaleLowerCase().replace(/\s+/g, " ").trim();
  const groups = new Map<string, SeriesBook[]>();
  for (const book of books) {
    const title = normalize(book.effectiveTitle);
    const identity = title ? JSON.stringify([title, normalize(book.effectiveCreator), normalize(book.effectiveVolume)]) : `book:${book.id}`;
    const copies = groups.get(identity) ?? [];
    if (!copies.some(copy => copy.id === book.id)) copies.push(book);
    groups.set(identity, copies);
  }
  return [...groups.values()].map(copies => [...copies].sort((a, b) => Number(b.isAvailable) - Number(a.isAvailable) || a.id - b.id));
}
