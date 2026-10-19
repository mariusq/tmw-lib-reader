import { getCurrentWindow } from "@tauri-apps/api/window";
import { captureCharacterPage } from "../../../packages/reader-core/readerCharacters";
import { localDay } from "../../../packages/reader-core/readingActivity";
import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { readingIntervals } from "../../../packages/reader-core/readingActivity";
import type { Rendition } from "epubjs";
import type { RefObject } from "react";

export function useReadingActivity(
  bookId: number,
  ready: boolean,
  rendition: RefObject<Rendition | null>,
  onError: (message: string) => void,
  contentKey?: string,
  fixedLayout?: RefObject<boolean>,
) {
  const prepareAdvance = useRef<() => () => void>(() => () => {});
  useEffect(() => {
    if (!ready || !rendition.current) return;
    let wall = Date.now(),
      mono = performance.now(),
      lastInput = mono;
    let nativeFocused: boolean | null = null;
    const isFocused = () => document.visibilityState === "visible" && (nativeFocused ?? document.hasFocus());
    let focused = isFocused();
    let queue = Promise.resolve();
    let generation: string | undefined;
    let generationReady = false;
    const refreshGeneration = () => {
      void invoke<unknown>("get_reading_activity_generation").then(value => {
        if (disposed) return;
        const next = typeof value === "string" ? value : undefined;
        if (generation !== next) pageMilliseconds = 0;
        generation = next; generationReady = true;
      }).catch(reason => onError(`Could not read activity generation: ${String(reason)}`));
    };
    let pageMilliseconds = 0;
    let disposed = false;
    const bound = new Set<Document>();
    const tick = () => {
      const now = Date.now(),
        current = performance.now();
      const delta = current - mono;
      const elapsed = Math.min(delta, Math.max(0, lastInput + 1800000 - mono));
      if (generationReady && focused && delta <= 15000 && Math.abs(now - wall - delta) <= 1000)
        for (const interval of readingIntervals(wall, now - (delta - elapsed), elapsed)) {
          pageMilliseconds += interval.milliseconds;
          const capturedGeneration = generation;
          queue = queue
            .then(async () => {
              // Idempotent persisted watermark makes one retry safe.
              try {
                await invoke("record_reading_activity", { request: { bookId, ...interval, ...(capturedGeneration ? {generation: capturedGeneration} : {}) } });
              } catch {
                await invoke("record_reading_activity", { request: { bookId, ...interval, ...(capturedGeneration ? {generation: capturedGeneration} : {}) } });
              }
            })
            .catch((reason) => onError(`Could not save reading activity: ${String(reason)}`));
        }
      wall = now;
      mono = current;
    };
    const view = rendition.current;
    prepareAdvance.current = () => {
      tick();
      const page =
        contentKey &&
        !fixedLayout?.current &&
        focused &&
        pageMilliseconds >= 3000 &&
        performance.now() - lastInput <= 1800000
          ? captureCharacterPage(view)
          : null;
      const day = localDay(new Date());
      const capturedGeneration = generation;
      return () => {
        const after = view.currentLocation()?.start?.cfi;
        if (disposed || rendition.current !== view || !page || !after || after === page.cfi) return;
        queue = queue
          .then(async () => {
            const request = {
              bookId,
              ...(capturedGeneration ? {generation: capturedGeneration} : {}),
              contentKey,
              day,
              spine: page.spine,
              startOffset: page.startOffset,
              endOffset: page.endOffset,
            };
            try {
              await invoke("record_reading_characters", { request });
            } catch {
              await invoke("record_reading_characters", { request });
            }
          })
          .catch((reason) => onError(`Could not save character estimate: ${String(reason)}`));
      };
    };
    const relocated = () => {
      tick();
      pageMilliseconds = 0;
    };
    view.on("relocated", relocated);
    const input = () => {
      tick();
      lastInput = performance.now();
    };
    const updateFocus = () => {
      tick();
      const next = isFocused();
      if (next && !focused) lastInput = performance.now();
      focused = next;
    };
    // DOM window blur also fires when focus enters an EPUB iframe. Native window
    // focus distinguishes that from switching apps or minimizing the reader.
    let unlistenFocus: (() => void) | undefined;
    let focusRevision = 0;
    try {
      const appWindow = getCurrentWindow();
      void appWindow.onFocusChanged(event => {
        if (disposed) return;
        tick(); nativeFocused = event.payload; ++focusRevision;
        const next = isFocused();
        if (next && !focused) lastInput = performance.now();
        focused = next;
      }).then(unlisten => { if (disposed) unlisten(); else unlistenFocus = unlisten; }).catch(() => {});
      const revision = focusRevision;
      void appWindow.isFocused().then(value => {
        if (disposed || focusRevision !== revision) return;
        tick(); nativeFocused = value; focused = isFocused();
      }).catch(() => {});
    } catch { /* Browser-only tests/previews use document focus as a fallback. */ }
    const visibility = updateFocus;
    const events = ["pointerdown", "keydown", "wheel", "touchstart"] as const;
    const bind = (doc: Document) => {
      if (bound.has(doc)) return;
      bound.add(doc);
      events.forEach((event) =>
        doc.addEventListener(event, input, { passive: true, capture: true }),
      );
    };
    const bindContents = () => {
      const visible = new Set(
        (rendition.current?.getContents?.() ?? []).map((content) => content.document),
      );
      // Release old spine documents rather than retaining an entire book through listeners.
      for (const doc of bound)
        if (doc !== document && !visible.has(doc)) {
          events.forEach((event) => doc.removeEventListener(event, input, true));
          bound.delete(doc);
        }
      visible.forEach(bind);
    };
    bind(document);
    bindContents();
    view?.on("rendered", bindContents);
    window.addEventListener("blur", updateFocus);
    window.addEventListener("focus", updateFocus);
    document.addEventListener("visibilitychange", visibility);
    refreshGeneration();
    const generationTimer = window.setInterval(refreshGeneration, 30000);
    const timer = window.setInterval(tick, 10000);
    return () => {
      disposed = true;
      unlistenFocus?.();
      tick();
      prepareAdvance.current = () => () => {};
      view.off?.("relocated", relocated);
      clearInterval(timer);
      clearInterval(generationTimer);
      view?.off?.("rendered", bindContents);
      window.removeEventListener("blur", updateFocus);
      window.removeEventListener("focus", updateFocus);
      document.removeEventListener("visibilitychange", visibility);
      bound.forEach((doc) =>
        events.forEach((event) => doc.removeEventListener(event, input, true)),
      );
    };
  }, [bookId, ready, rendition, onError, contentKey, fixedLayout]);
  return prepareAdvance;
}
