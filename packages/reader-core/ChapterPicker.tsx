import { useEffect, useRef } from "react";

export type Chapter = { label: string; href: string; subitems?: Chapter[] };

export default function ChapterPicker({ chapters, ready, pending, onJump, overlay = false, background }: {
  chapters: Chapter[];
  ready: boolean;
  pending: boolean;
  onJump: (href: string) => void;
  overlay?: boolean;
  background?: string;
}) {
  const picker = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    if (!overlay) return;
    const dismiss = (event: PointerEvent) => {
      if (picker.current && !picker.current.contains(event.target as Node)) picker.current.open = false;
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [overlay]);
  function entries(items: Chapter[]) {
    return <ul style={{ listStyle: "none", paddingInlineStart: "1rem" }}>
      {items.map((item, index) => <li key={`${item.href}:${index}`}>
        {item.href && !/^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(item.href.trim()) ?
          <button type="button" disabled={pending} onClick={() => {
            onJump(item.href);
            if (overlay && picker.current) picker.current.open = false;
          }}
            style={{ textAlign: "start", padding: "0.6rem", width: "100%", overflowWrap: "anywhere" }}>
            {item.label.trim() || "Untitled chapter"}
          </button> : <span>{item.label.trim() || "Untitled chapter"}</span>}
        {item.subitems?.length ? entries(item.subitems) : null}
      </li>)}
    </ul>;
  }
  return <details ref={picker} className={overlay ? "reader-chapter-picker" : undefined}
    onKeyDown={(event) => {
      if (overlay && event.key === "Escape" && picker.current?.open) {
        event.preventDefault();
        event.stopPropagation();
        picker.current.open = false;
        picker.current.querySelector("summary")?.focus();
      }
    }}>
    <summary className="reader-control control">Go to chapter</summary>
    <nav aria-label="Chapters" style={{ backgroundColor: background, maxHeight: "45vh", overflowY: "auto", minWidth: "12rem", maxWidth: "28rem" }}>
      {!ready ? <p>Loading chapters…</p> : chapters.length ? entries(chapters) :
        <p>This book has no table of contents.</p>}
    </nav>
  </details>;
}
