import { createElement, Fragment, type ReactNode } from "react";
import type { ChunkDictionaryEntry, ChunkLookupResponse, DictionaryMetadata } from "./dictionaryLookup";

const tags = new Set(["ruby", "rt", "rp", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "span", "div", "ol", "ul", "li", "details", "summary"]);
const MAX_NODES = 4096;
const MAX_DEPTH = 32;
type Budget = { remaining: number };
function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

export function adaptChunkLookup(response: ChunkLookupResponse) {
  return {
    contractVersion: 1 as const,
    engineVersion: response.engineVersion,
    target: response.target,
    matchedSpan: response.matchedSpan,
    groups: response.groups,
    entries: response.groups.flatMap(group => group.matches.map(match => ({
      ...match.entry,
      reading: match.entry.reading || null,
      definitions: glossaryText(match.entry.glossary),
      partOfSpeech: match.entry.rules,
      dictionaryName: match.entry.provenance.title,
      matchedSpan: match.matchedSpan,
    }))),
  };
}

/** Text snapshots for saved passages/history; never interpret dictionary HTML. */
export function glossaryText(glossary: unknown): string[] {
  const budget = { remaining: MAX_NODES };
  function plain(value: unknown, depth: number): string {
    if (budget.remaining-- <= 0 || depth > MAX_DEPTH) return "[Definition truncated]";
    if (typeof value === "string") return value;
    if (Array.isArray(value)) return value.map(item => plain(item, depth + 1)).join(" ");
    const node = object(value);
    if (!node) return "";
    if (node.type === "text") return typeof node.text === "string" ? node.text : "";
    if (node.tag === "br") return "\n";
    if (node.tag === "img" || node.type === "image") return typeof node.description === "string" ? node.description : typeof node.alt === "string" ? node.alt : "[Local image]";
    return `${node.unsupported ? "[Unsupported definition element] " : ""}${plain(node.content, depth + 1)}`;
  }
  return (Array.isArray(glossary) ? glossary : [glossary]).map(value => plain(value, 0)).filter(Boolean);
}

function renderNode(value: unknown, assets: Record<string, string>, budget: Budget, depth = 0): ReactNode {
  if (budget.remaining-- <= 0 || depth > MAX_DEPTH) return <span>[Definition truncated]</span>;
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.slice(0, MAX_NODES).map((child, index) => <Fragment key={index}>{renderNode(child, assets, budget, depth + 1)}</Fragment>);
  const node = object(value);
  if (!node) return null;
  const content = () => renderNode(node.content, assets, budget, depth + 1);
  if (node.type === "text") return typeof node.text === "string" ? node.text : null;
  if (node.type === "structured-content") return content();
  if (node.unsupported) return <span><small role="note">[Unsupported definition element]</small>{content()}</span>;
  if (node.tag === "br") return <br />;
  if (node.tag === "img" || node.type === "image") {
    const path = typeof node.path === "string" ? node.path : "";
    const src = Object.prototype.hasOwnProperty.call(assets, path) ? assets[path] : "";
    const alt = typeof node.description === "string" ? node.description : typeof node.alt === "string" ? node.alt : "Dictionary image";
    // Only backend-supplied bounded local images; never URLs, SVG or HTML.
    if (!/^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/]+=*$/.test(src)) return <span role="note">[Local image unavailable: {alt}]</span>;
    return <img src={src} alt={alt} loading="lazy" style={{ maxWidth: "100%", maxHeight: "24rem", objectFit: "contain" }} />;
  }
  if (node.tag === "a") return <span>{content()}<small role="note"> [Dictionary link disabled]</small></span>;
  if (typeof node.tag !== "string" || !tags.has(node.tag)) return <span><small role="note">[Unsupported definition element]</small>{content()}</span>;
  return createElement(node.tag, {
    title: typeof node.title === "string" ? node.title : undefined,
    lang: typeof node.lang === "string" ? node.lang : undefined,
  }, content());
}

function metadataText(item: DictionaryMetadata): string {
  const data = object(item.data);
  if (item.mode === "freq") {
    const frequency = data?.frequency ?? item.data;
    const frequencyObject = object(frequency);
    const value = frequencyObject?.displayValue ?? frequencyObject?.value ?? frequency;
    return `Frequency: ${typeof value === "string" || typeof value === "number" ? value : "unsupported metadata"}${typeof data?.reading === "string" ? ` (${data.reading})` : ""}`;
  }
  if (item.mode === "pitch" && Array.isArray(data?.pitches)) {
    const pitches = data.pitches.slice(0, 64).map(value => {
      const pitch = object(value);
      const position = pitch?.position;
      if (!(typeof position === "number" || (typeof position === "string" && /^[HL]{1,256}$/.test(position)))) return "unsupported pitch";
      const labels = [String(position)];
      for (const [key, label] of [["nasal", "nasal"], ["devoice", "devoiced"], ["tags", "tags"]] as const) {
        const values = pitch?.[key];
        if (Array.isArray(values)) {
          const details = values.slice(0, 64).filter(value => typeof value === "string" || typeof value === "number");
          if (details.length) labels.push(`${label}: ${details.join(", ")}`);
        }
      }
      return labels.join("; ");
    });
    return `Pitch accent: ${pitches.length ? pitches.join(" / ") : "unsupported metadata"}${typeof data?.reading === "string" ? ` (${data.reading})` : ""}`;
  }
  return `Unsupported metadata: ${item.mode}`;
}

/** React constructs the allowlisted tree: rich definitions never enter innerHTML. */
export default function DictionaryGlossary({ entry }: { entry: ChunkDictionaryEntry }) {
  const definitions = Array.isArray(entry.glossary) ? entry.glossary : [entry.glossary];
  const budget = { remaining: MAX_NODES };
  const labels = [...new Set([...entry.termTags, ...entry.definitionTags])];
  return <div className="dictionary-glossary">
    {labels.length > 0 && <p className="text-xs opacity-70">{labels.join(" · ")}</p>}
    <ol>{definitions.slice(0, MAX_NODES).map((definition, index) => <li key={index}>{renderNode(definition, entry.assets ?? {}, budget)}</li>)}</ol>
    {entry.metadata?.slice(0, 64).map((item, index) => <p className="text-xs opacity-70" key={index}>{item.title}: {metadataText(item)}</p>)}
  </div>;
}
