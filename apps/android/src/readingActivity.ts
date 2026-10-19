import { invoke } from "@tauri-apps/api/core";
import type { Rendition } from "epubjs";
import type { LocalBook } from "./localBook";
import { ActivityClock } from "../../../packages/reader-core/activityClock";
import { captureCharacterPage, type CharacterRendition } from "../../../packages/reader-core/readerCharacters";
import { localDay } from "../../../packages/reader-core/readingActivity";

export const activityNative = <T,>(args: Record<string, unknown>) => invoke<T>("mobile_storage", { args });
type Session = { token: string; foreground: boolean; enabled: boolean };
export function activityIdentity(book?: LocalBook) {
  return book?.catalog ?? { ns: "local-proof", localBookId: book?.id ?? "", version: book?.id ? `sha256-${book.id}` : "" };
}
export function prepareActivityLookup(book?: LocalBook) {
  const identity = activityIdentity(book);
  const session = activityNative<Session>({ action: "activityStart", ...identity });
  // Attach a rejection handler immediately, even if the lookup later fails.
  const captured = session.catch(() => null);
  return async () => {
    const state = await captured;
    if (!state?.enabled) return;
    await activityNative({ action: "activityRecord", ...identity, token: state.token, title: book?.name ?? "Dictionary",
      event: { id: crypto.randomUUID(), kind: "lookup", day: localDay(new Date()), occurredAt: Date.now() } });
  };
}

/** Starts only after the rendition has successfully displayed. One bounded native writer. */
export function trackAndroidReading(view: Rendition, book: LocalBook, fixed: boolean, onError: (message: string) => void) {
  const identity = activityIdentity(book);
  const characterView = view as unknown as CharacterRendition;
  const clock = new ActivityClock(Date.now(), performance.now(), false);
  let nativeForeground = true, disposed = false, ready = false, enabled = true, token = "", revision = 0, queued = 0;
  let queue = Promise.resolve();
  const docs = new Set<Document>();
  const write = (event: Record<string, unknown>) => {
    if (!ready || queued >= 6) return;
    const capturedToken = token;
    queued++;
    queue = queue.then(async () => {
      const args = { action: "activityRecord", ...identity, token: capturedToken, title: book.name, event };
      try { await activityNative(args); } catch { await activityNative(args); }
    }).catch(reason => onError(`Activity save failed: ${String(reason)}`)).finally(() => { queued--; });
  };
  const tick = () => {
    for (const interval of clock.sample(Date.now(), performance.now())) write({ id: crypto.randomUUID(), kind: "time", occurredAt: Date.now(), ...interval });
  };
  const foreground = () => {
    tick(); clock.focus(ready && enabled && nativeForeground && document.visibilityState === "visible", performance.now());
  };
  const lifecycle = (event: Event) => {
    const state = (event as CustomEvent<string>).detail;
    revision++;
    nativeForeground = state === "resume" || state === "foreground";
    foreground();
  };
  const refresh = () => {
    const current = revision;
    void activityNative<Session>({ action: "activityStart", ...identity }).then(session => {
      if (disposed) return;
      tick();
      if (token && token !== session.token) { clock.page(); revision++; }
      token = session.token; ready = true; enabled = session.enabled;
      if (current === revision) nativeForeground = session.foreground;
      foreground();
    }).catch(reason => onError(`Activity unavailable: ${String(reason)}`));
  };
  const input = () => { tick(); clock.input(performance.now()); };
  const events = ["pointerdown", "keydown", "wheel", "touchstart"] as const;
  const bind = (doc: Document) => { if (docs.has(doc)) return; docs.add(doc); events.forEach(event => doc.addEventListener(event, input, { passive: true, capture: true })); };
  const bindContents = () => {
    const visible = new Set<Document>(((view.getContents?.() ?? []) as unknown as { document: Document }[]).map(content => content.document));
    for (const doc of docs) if (doc !== document && !visible.has(doc)) { events.forEach(event => doc.removeEventListener(event, input, true)); docs.delete(doc); }
    bind(document); visible.forEach(bind);
  };
  const relocated = () => { tick(); clock.page(); };
  const visibility = () => { revision++; foreground(); };
  view.on("relocated", relocated); view.on("rendered", bindContents); bindContents();
  window.addEventListener("tmw-lifecycle", lifecycle);
  document.addEventListener("visibilitychange", visibility);
  window.addEventListener("pagehide", visibility);
  window.addEventListener("tmw-activity-settings", refresh);
  refresh();
  const timer = setInterval(tick, 10000);
  const refreshTimer = setInterval(refresh, 30000);
  return {
    prepare() {
      tick();
      const page = !fixed && identity.version && clock.canCredit(performance.now()) ? captureCharacterPage(characterView) : null;
      const day = localDay(new Date());
      const capturedRevision = revision;
      return () => {
        const after = captureCharacterPage(characterView);
        if (disposed || revision !== capturedRevision || !page || !after || after.cfi === page.cfi || after.spine < page.spine ||
          (after.spine === page.spine && after.startOffset <= page.startOffset)) return;
        write({ id: crypto.randomUUID(), kind: "characters", day, occurredAt: Date.now(), spine: page.spine, startOffset: page.startOffset, endOffset: page.endOffset });
      };
    },
    invalidate() { tick(); revision++; clock.page(); },
    stop() {
      tick(); disposed = true;
      clearInterval(timer); clearInterval(refreshTimer);
      view.off?.("relocated", relocated); view.off?.("rendered", bindContents);
      window.removeEventListener("tmw-lifecycle", lifecycle); window.removeEventListener("pagehide", visibility);
      window.removeEventListener("tmw-activity-settings", refresh); document.removeEventListener("visibilitychange", visibility);
      docs.forEach(doc => events.forEach(event => doc.removeEventListener(event, input, true)));
    },
  };
}
