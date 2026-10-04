export type DictionaryTextTarget = {
  text: string;
  offset: number;
};

/**
 * Produces the text sent to the Japanese tokenizer without ruby annotations.
 * `textContent` cannot be used here because it flattens
 * `<ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>` into `漢かん字じ`.
 */
export function dictionaryTextAt(
  root: Element,
  targetNode: Text,
  targetOffset: number,
): DictionaryTextTarget | null {
  const document = root.ownerDocument;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = (node as Text).parentElement;
      return parent?.closest("rt, rp, script, style")
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT;
    },
  });

  let text = "";
  let offset: number | null = null;
  let node = walker.nextNode() as Text | null;
  while (node) {
    if (node === targetNode) {
      offset = [...text].length + [...node.data.slice(0, targetOffset)].length;
    }
    text += node.data;
    node = walker.nextNode() as Text | null;
  }

  if (offset === null) return null;
  const characterCount = [...text].length;
  if (characterCount === 0) return null;
  return { text, offset: Math.min(offset, characterCount - 1) };
}

export function japaneseWordAt(value: string, offset: number): string {
  for (const match of value.matchAll(/[\u3040-\u30ff\u3400-\u9fff々〆ヶ]+/gu)) {
    const word = match[0];
    const matchStart = [...value.slice(0, match.index)].length;
    const matchEnd = matchStart + [...word].length;
    if (offset >= matchStart && offset <= matchEnd) return [...word].slice(0, 32).join("");
  }
  return "";
}
