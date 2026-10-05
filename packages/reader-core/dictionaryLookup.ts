/** Contract v1. All offsets are Unicode scalar values; span end is exclusive.
 * Reader sentence/CFI and history identity remain owned by platform adapters.
 */
export type LookupRequest = { text: string; offset: number };
export type DictionaryTarget = { surface: string; lemma: string; reading: string | null };
export type DictionaryEntry = {
  /** Ephemeral database ID, never portable saved-word identity. */
  id: number;
  term: string;
  reading: string | null;
  definitions: string[];
  partOfSpeech: string[];
  dictionaryName: string;
};
export type LookupResponse = {
  contractVersion: 1;
  target: DictionaryTarget;
  matchedSpan: { start: number; end: number };
  entries: DictionaryEntry[];
};

export type DictionaryProvenance = { source: string; title: string; revision: string };
export type DictionaryMetadata = { source: string; title: string; mode: string; data: unknown };
export type ChunkDictionaryEntry = {
  id: number;
  term: string;
  reading: string;
  provenance: DictionaryProvenance;
  rules: string[];
  glossary: unknown;
  definitionTags: string[];
  termTags: string[];
  sequence: number;
  score: number;
  priority: number;
  assets?: Record<string, string>;
  metadata?: DictionaryMetadata[];
};
export type ChunkMatch = {
  entry: ChunkDictionaryEntry;
  matchedSpan: { start: number; end: number };
  deinflectionDepth: number;
};
export type ChunkLookupResponse = {
  engineVersion: number;
  target: DictionaryTarget;
  matchedSpan: { start: number; end: number };
  groups: { term: string; reading: string; matches: ChunkMatch[] }[];
};

/** Keep portable source identity within the existing v3 history wire limit. */
export async function portableDictionaryId(provenance: DictionaryProvenance): Promise<string> {
  const identity = `${provenance.source}:${provenance.revision}`;
  if (identity.length <= 128) return identity;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify([provenance.source, provenance.revision])));
  return `dictionary-sha256:${Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("")}`;
}
