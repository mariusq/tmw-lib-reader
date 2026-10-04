export function readerRenditionOptions(): Record<string, unknown> {
  // Leaving flow, layout, spread, and defaultDirection unset is intentional:
  // epub.js derives them from the EPUB package metadata.
  return { width: "100%", height: "100%", manager: "default" };
}
