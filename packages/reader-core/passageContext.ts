/** Offsets use Unicode code points, matching the dictionary tokenizer. */
export function sentenceAt(text: string, offset: number): string {
  const characters = [...text];
  const position = Math.max(0, Math.min(offset, characters.length - 1));
  const boundary = /[。！？!?\n]/u;
  let start = position;
  while (start > 0 && !boundary.test(characters[start - 1])) start--;
  let end = position;
  while (end < characters.length && !boundary.test(characters[end])) end++;
  // An unusually long paragraph must still retain the clicked word.
  start = Math.max(start, position - 2000);
  return characters
    .slice(start, Math.min(end + 1, characters.length, start + 4000))
    .join("")
    .trim();
}
