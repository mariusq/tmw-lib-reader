import type { CSSProperties } from "react";

const cjk = /[぀-ヿ㐀-鿿豈-﫿ｦ-ﾟ]/;

// Stable per-title hue so a book keeps the same placeholder colour across views and restarts.
function hue(title: string): number {
  let hash = 0;
  for (const char of title) hash = (hash * 31 + char.codePointAt(0)!) >>> 0;
  return hash % 360;
}

/** Placeholder cover used when covers are hidden or a book has no usable cover image. */
export function GeneratedCover({ title }: { title: string }) {
  const text = title.trim();
  const h = hue(text || "?");
  return (
    <div
      className={`generated-cover${cjk.test(text) ? " generated-cover-vertical" : ""}`}
      style={{ "--cover-hue": h } as CSSProperties}
      aria-hidden="true"
    >
      <span>{text || "本"}</span>
    </div>
  );
}
