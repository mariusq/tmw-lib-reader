/** Resolve the glyph under the point, rather than the insertion boundary after it.
 * Range rectangles work in horizontal and vertical writing modes.
 */
export function caretAt(
  document: Document,
  x: number,
  y: number,
  glyphHit = true,
): { node: Text; offset: number } | null {
  const dom = document as Document & {
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
  };
  const legacy = dom.caretRangeFromPoint?.(x, y);
  const modern = legacy ? null : dom.caretPositionFromPoint?.(x, y);
  const node = legacy?.startContainer ?? modern?.offsetNode;
  const offset = legacy?.startOffset ?? modern?.offset;
  if (!node || node.nodeType !== 3 || offset === undefined) return null;
  const text = node as Text;
  if (!glyphHit) return { node: text, offset };
  let start = 0;
  for (const character of text.data) {
    const end = start + character.length;
    if (Math.abs(start - offset) <= 2 || Math.abs(end - offset) <= 2) {
      const range = document.createRange();
      range.setStart(text, start);
      range.setEnd(text, end);
      for (const rect of range.getClientRects()) {
        if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)
          return { node: text, offset: start };
      }
    }
    start = end;
  }
  // A point in the margin is not a word tap.
  return null;
}
