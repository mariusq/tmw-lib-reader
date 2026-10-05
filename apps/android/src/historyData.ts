import { portableDictionaryId } from "../../../packages/reader-core/dictionaryLookup";
import { mobileUser } from "./userData";
import type { LocalBook } from "./localBook";
import type { ChunkLookupResponse, ChunkDictionaryEntry, DictionaryEntry, LookupResponse } from "../../../packages/reader-core/dictionaryLookup";
import { adaptChunkLookup } from "../../../packages/reader-core/DictionaryGlossary";

// History accepts the stable subset, including older stored/test responses.
export type LookupResult = Pick<LookupResponse, "target"> & {
  entries: (Pick<DictionaryEntry, "term" | "reading" | "definitions"> & { provenance?: ChunkDictionaryEntry["provenance"] })[];
  groups?: ChunkLookupResponse["groups"];
  elapsedMs: number;
  dictionaryBytes: number;
};
export function readerLookupResult(response: LookupResult | (ChunkLookupResponse & { elapsedMs: number; dictionaryBytes: number })): LookupResult {
  if (!("groups" in response) || !response.groups) return response as LookupResult;
  return {
    ...response,
    ...adaptChunkLookup(response as ChunkLookupResponse),
  };
}
export type HistoryFields = {
  surface: string;
  headword: string | null;
  reading: string | null;
  sentence: string;
  locationCfi: string;
  dictionaryId: string;
  dictionaryEntryId: string | null;
  lookedUpAt: string;
};
export type HistoryRow = {
  ns: string;
  bookId: string;
  entityId: string;
  contentVersion: string | null;
  localBookId?: string;
  fields: HistoryFields;
};
export type HistoryPage = {
  rows: HistoryRow[];
  next: number | null;
  settings: { enabled: boolean; retentionLimit: number };
  pending: number;
};
export const DICTIONARY_ID =
  "jmdict-eng:20260928:5f54504a62a7f45741e1bf6fd28f6e1e5add6405f2829748cc004a802839a3aa";
const bounded = (value: string, size: number) => value.slice(0, size).replace(/[\uD800-\uDBFF]$/, "");

/** Called after displaying a completed lookup; native storage never blocks definitions. */
export async function recordLookup(book: LocalBook, result: LookupResult, sentence: string, cfi: string) {
  const entry = result.entries[0];
  return mobileUser<{ recorded: boolean }>({
    action: "historyRecord",
    ...book.catalog,
    localBookId: book.id ?? "legacy",
    fields: {
      surface: bounded(result.target.surface, 256),
      headword: entry ? bounded(entry.term, 256) : null,
      reading: entry?.reading ? bounded(entry.reading, 256) : null,
      sentence: bounded(sentence, 4000),
      locationCfi: bounded(cfi, 4096),
      dictionaryId: entry?.provenance ? await portableDictionaryId(entry.provenance) : DICTIONARY_ID,
      dictionaryEntryId: null,
      lookedUpAt: String(Math.floor(Date.now() / 1000)),
    } satisfies HistoryFields,
  });
}
export function canJumpHistory(row: HistoryRow, book?: LocalBook) {
  if (!book || !row.fields.locationCfi) return false;
  if (book.catalog)
    return row.ns === book.catalog.ns && row.bookId === book.catalog.id &&
      row.contentVersion === book.catalog.version;
  return !!book.id && row.localBookId === book.id;
}
