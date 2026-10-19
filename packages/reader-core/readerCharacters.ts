import type { PageLocation } from "./readerNavigation";
export type CharacterPage = { spine: number; startOffset: number; endOffset: number; cfi: string };
export type CharacterRendition = {
  currentLocation: () => PageLocation | undefined;
  getRange?: (cfi: string) => Range | undefined;
};
const indexes = new WeakMap<Document, { nodes: Text[]; prefixes: number[]; total: number }>();
export function readingCharacterCount(text: string): number {
  let count = 0;
  for (const scalar of text) if (!/\s/u.test(scalar)) count++;
  return count;
}
function indexDocument(doc: Document) {
  const cached = indexes.get(doc);
  if (cached) return cached;
  const walker = doc.createTreeWalker(doc.body ?? doc.documentElement, 4);
  const nodes: Text[] = [],
    prefixes: number[] = [];
  let total = 0,
    visited = 0, scannedUnits = 0;
  const hidden = new WeakMap<Element, boolean>();
  const isHidden = (element: Element | null): boolean => {
    const path: Element[] = [];
    let result = false;
    for (let current = element; current; current = current.parentElement) {
      const cached = hidden.get(current);
      if (cached !== undefined) { result = cached; break; }
      path.push(current);
      const style = doc.defaultView?.getComputedStyle(current);
      if (style?.display === "none" || style?.visibility === "hidden" || style?.visibility === "collapse" || style?.opacity === "0") { result = true; break; }
    }
    path.forEach(node => hidden.set(node, result));
    return result;
  };
  while (walker.nextNode()) {
    if (++visited > 100000) return null;
    const node = walker.currentNode as Text;
    scannedUnits += node.data.length;
    if (scannedUnits > 2000000) return null;
    if (
      !node.data ||
      node.parentElement?.closest('rt,rp,script,style,noscript,[hidden],[aria-hidden="true"]') ||
      isHidden(node.parentElement)
    )
      continue;
    nodes.push(node);
    prefixes.push(total);
    total += readingCharacterCount(node.data);
    if (total > 100000000) return null;
  }
  const index = { nodes, prefixes, total };
  indexes.set(doc, index);
  return index;
}
function offsetAt(point: Range): number | null {
  const doc = point.startContainer.ownerDocument;
  if (!doc) return null;
  const index = indexDocument(doc);
  if (!index) return null;
  for (let i = 0; i < index.nodes.length; i++) {
    const node = index.nodes[i];
    if (node === point.startContainer)
      return index.prefixes[i] + readingCharacterCount(node.data.slice(0, point.startOffset));
    if (point.comparePoint(node, 0) >= 0) return index.prefixes[i];
  }
  return index.total;
}
export function captureCharacterPage(view: CharacterRendition): CharacterPage | null {
  try {
    const location = view.currentLocation();
    const start = location?.start,
      end = location?.end;
    if (!start?.cfi || !end?.cfi || start.index !== end.index || !Number.isInteger(start.index))
      return null;
    const from = view.getRange?.(start.cfi),
      to = view.getRange?.(end.cfi);
    if (!from || !to || from.startContainer.ownerDocument !== to.startContainer.ownerDocument)
      return null;
    const startOffset = offsetAt(from),
      endOffset = offsetAt(to);
    if (
      startOffset === null ||
      endOffset === null ||
      endOffset <= startOffset ||
      endOffset - startOffset > 100000
    )
      return null;
    return { spine: start.index, startOffset, endOffset, cfi: start.cfi };
  } catch {
    return null;
  } // Unresolvable/image-only pages do not get invented text counts.
}
