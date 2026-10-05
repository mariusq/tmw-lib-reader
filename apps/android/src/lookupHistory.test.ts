import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { canJumpHistory, DICTIONARY_ID, recordLookup, type HistoryRow } from "./historyData";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({ recorded: true })) }));
const identity = { ns: "a".repeat(32), id: "b".repeat(32), version: `sha256-${"c".repeat(64)}` };
const book = { bytes: new ArrayBuffer(0), name: "Book", id: "c".repeat(64), catalog: identity };
const result = { target: { surface: "読んだ", lemma: "読む", reading: "ヨンダ" },
  entries: [{ term: "読む", reading: "よむ", definitions: ["read"] }], elapsedMs: 1, dictionaryBytes: 1 };
it("records bounded completed taps independently of saved passages with portable dictionary references", async () => {
  await recordLookup(book, result, "本".repeat(9000), "epubcfi(/6/2)");
  expect(invoke).toHaveBeenLastCalledWith("mobile_storage", { args: expect.objectContaining({
    action: "historyRecord", ...identity, fields: expect.objectContaining({
      surface: "読んだ", headword: "読む", reading: "よむ", sentence: "本".repeat(4000),
      dictionaryId: DICTIONARY_ID, dictionaryEntryId: null,
    }),
  }) });
});
it("keeps proof history local and permits jumps only into the exact known copy", async () => {
  const proof = { ...book, catalog: undefined };
  await recordLookup(proof, { ...result, entries: [] }, "本。", "epubcfi(/6/2)");
  expect(invoke).toHaveBeenLastCalledWith("mobile_storage", { args: expect.objectContaining({
    action: "historyRecord", localBookId: book.id, fields: expect.objectContaining({ headword: null }),
  }) });
  const row = { ns: identity.ns, bookId: identity.id, entityId: "d".repeat(32),
    contentVersion: identity.version, localBookId: book.id,
    fields: { locationCfi: "epubcfi(/6/2)" } } as HistoryRow;
  expect(canJumpHistory(row, book)).toBe(true);
  expect(canJumpHistory({ ...row, contentVersion: null }, book)).toBe(false);
  expect(canJumpHistory({ ...row, ns: "e".repeat(32) }, book)).toBe(false);
  expect(canJumpHistory(row, proof)).toBe(true);
  expect(canJumpHistory(row, { ...proof, id: undefined })).toBe(false);
});
it("retains imported dictionary source and revision without ephemeral row identity", async () => {
  await recordLookup(book, { ...result, entries: [{ ...result.entries[0], provenance: {
    source: "yomitan:Local dictionary", title: "Local dictionary", revision: "2026.10",
  } }] }, "本。", "epubcfi(/6/2)");
  expect(invoke).toHaveBeenLastCalledWith("mobile_storage", { args: expect.objectContaining({
    fields: expect.objectContaining({ dictionaryId: "yomitan:Local dictionary:2026.10", dictionaryEntryId: null }),
  }) });
});
