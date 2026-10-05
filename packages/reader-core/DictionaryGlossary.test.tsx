import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import DictionaryGlossary, { adaptChunkLookup, glossaryText } from "./DictionaryGlossary";
import { portableDictionaryId } from "./dictionaryLookup";
import { webcrypto } from "node:crypto";
import type { ChunkDictionaryEntry } from "./dictionaryLookup";

const entry: ChunkDictionaryEntry = {
  id: 2, term: "読む", reading: "よむ", provenance: { source: "test", title: "Test", revision: "1" },
  glossary: [], rules: ["v5"], definitionTags: ["common"], termTags: ["common"], sequence: 1, score: 0, priority: 0,
};
describe("safe dictionary glossary", () => {
  it("renders ruby/table content without executing HTML or remote resources", () => {
    const { container } = render(<DictionaryGlossary entry={{ ...entry, glossary: [
      "<script>evil()</script>", { type: "structured-content", content: [
        { tag: "ruby", content: ["漢字", { tag: "rt", content: "かんじ" }] },
        { tag: "table", content: [{ tag: "tr", content: [{ tag: "td", content: "meaning" }] }] },
        { tag: "a", href: "https://example.com", content: "link" },
        { tag: "script", content: "blocked" }, { type: "image", path: "https://example.com/x.png", alt: "remote" },
      ] },
    ] }} />);
    expect(container.querySelector("script,a")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("ruby rt")?.textContent).toBe("かんじ");
    expect(container.querySelector("table td")?.textContent).toBe("meaning");
    expect(screen.getByText("[Unsupported definition element]")).toBeInTheDocument();
  });
  it("uses supplied safe local image bytes and renders frequency/pitch metadata", () => {
    render(<DictionaryGlossary entry={{ ...entry, glossary: [{ type: "image", path: "image.png", description: "diagram" }],
      assets: { "image.png": "data:image/png;base64,aGVsbG8=" }, metadata: [
        { source: "freq", title: "Frequency source", mode: "freq", data: { reading: "よむ", frequency: { value: 12, displayValue: "12★" } } },
        { source: "pitch", title: "Pitch source", mode: "pitch", data: { reading: "よむ", pitches: [{ position: 1 }, { position: "HL", nasal: [1], devoice: [2], tags: ["common"] }] } },
      ] }} />);
    expect(screen.getByAltText("diagram")).toHaveAttribute("src", "data:image/png;base64,aGVsbG8=");
    expect(screen.getByText(/Frequency: 12★/)).toBeInTheDocument();
    expect(screen.getByText(/Pitch accent: 1/)).toBeInTheDocument();
    expect(screen.getByText(/HL; nasal: 1; devoiced: 2; tags: common/)).toBeInTheDocument();
  });
  it("preserves portable source identity and alternate readings in adapters", () => {
    const response = adaptChunkLookup({ engineVersion: 1, target: { surface: "読んだ", lemma: "読む", reading: "よむ" }, matchedSpan: { start: 1, end: 4 }, groups: [
      { term: "読む", reading: "よむ", matches: [{ entry: { ...entry, glossary: [{ type: "structured-content", content: [{ tag: "span", content: "read" }] }] }, matchedSpan: { start: 1, end: 4 }, deinflectionDepth: 1 }] },
    ] });
    expect(response.entries[0].definitions).toEqual(["read"]);
    expect(response.entries[0].provenance.source).toBe("test");
    expect(response.groups[0].reading).toBe("よむ");
    expect(glossaryText([{ tag: "img", description: "illustration" }])).toEqual(["illustration"]);
  });
});

it("keeps portable source IDs bounded without merging long dictionary names", async () => {
  const original = globalThis.crypto;
  Object.defineProperty(globalThis,"crypto",{value:webcrypto,configurable:true});
  try {
    expect(await portableDictionaryId(entry.provenance)).toBe("test:1");
    const first={source:"yomitan:"+"a".repeat(4096),title:"long",revision:"1"};
    const id=await portableDictionaryId(first);
    expect(id.length).toBeLessThanOrEqual(128);
    expect(await portableDictionaryId(first)).toBe(id);
    expect(await portableDictionaryId({...first,revision:"2"})).not.toBe(id);
  } finally {Object.defineProperty(globalThis,"crypto",{value:original,configurable:true});}
});
