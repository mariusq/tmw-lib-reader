import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Rendition } from "epubjs";
import { ActivityClock } from "../../../packages/reader-core/activityClock";
import { trackAndroidReading, prepareActivityLookup } from "./readingActivity";
import { calendarDays, localDay, readingIntervals } from "../../../packages/reader-core/readingActivity";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
let mono = 0;
beforeEach(() => {
  vi.useFakeTimers(); vi.setSystemTime(new Date(2026, 9, 14, 12)); mono = 0;
  vi.spyOn(performance, "now").mockImplementation(() => mono);
  vi.mocked(invoke).mockReset().mockResolvedValue({ token: "generation", foreground: true, enabled: true });
});
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });
const events = () => vi.mocked(invoke).mock.calls.flatMap(([, input]) => {
  const args = (input as { args?: { action: string; event?: Record<string, unknown> } })?.args;
  return args?.action === "activityRecord" && args.event ? [args.event] : [];
});
const reader = () => ({ getContents: () => [], on: vi.fn(), off: vi.fn() }) as unknown as Rendition;
async function advance(ms: number) { mono += ms; await vi.advanceTimersByTimeAsync(ms); }
it("records 30 minutes of slow reading, ignores iframe blur, then idles", async () => {
  const tracker = trackAndroidReading(reader(), { name: "fixture", bytes: new ArrayBuffer(0) }, false, vi.fn());
  await Promise.resolve(); await Promise.resolve();
  window.dispatchEvent(new Event("blur"));
  for (let i = 0; i < 180; i++) await advance(10000);
  expect(events().reduce((sum, event) => sum + Number(event.milliseconds ?? 0), 0)).toBe(1800000);
  await advance(10000);
  expect(events()).toHaveLength(180);
  tracker.stop();
});
it("pauses on native foreground loss, lock/background and visibility; resume resets the clock", async () => {
  const tracker = trackAndroidReading(reader(), { name: "fixture", bytes: new ArrayBuffer(0) }, false, vi.fn());
  await Promise.resolve(); await Promise.resolve();
  await advance(10000);
  window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "background" }));
  await advance(10000);
  window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
  await advance(10000);
  expect(events()).toHaveLength(1);
  window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "resume" }));
  await advance(10000);
  expect(events()).toHaveLength(2);
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  document.dispatchEvent(new Event("visibilitychange")); await advance(10000);
  expect(events()).toHaveLength(2); tracker.stop();
});
it("drops sleep and wall-clock gaps and clears page dwell", () => {
  const clock = new ActivityClock(10000, 0, true);
  expect(clock.sample(20000, 10000)[0].milliseconds).toBe(10000);
  expect(clock.canCredit(10000)).toBe(true);
  expect(clock.sample(80000, 70000)).toEqual([]); expect(clock.dwell).toBe(0);
  expect(clock.sample(140000, 80000)).toEqual([]); expect(clock.dwell).toBe(0);
  clock.focus(false, 80000); expect(clock.sample(150000, 90000)).toEqual([]);
  clock.focus(true, 90000); expect(clock.sample(160000, 100000)[0].milliseconds).toBe(10000);
});
it("splits midnight and keeps DST calendar dates unique", () => {
  const before = new Date(2026, 9, 24, 23, 59, 55).getTime();
  const rows = readingIntervals(before, before + 10000, 10000);
  expect(rows.map(row => row.day)).toEqual(["2026-10-24", "2026-10-25"]);
  expect(rows.reduce((sum, row) => sum + row.milliseconds, 0)).toBe(10000);
  const days = calendarDays(new Date(2026, 9, 27), 7).map(localDay);
  expect(new Set(days).size).toBe(7);
  // Explicit offset transition: ten elapsed seconds cross a repeated local hour.
  const dst = Date.parse("2026-10-25T02:59:55+02:00");
  expect(readingIntervals(dst, dst + 10000, 10000).reduce((sum, row) => sum + row.milliseconds, 0)).toBe(10000);
});
it("successful intentional lookups use a captured generation and bypass history settings", async () => {
  const credit = prepareActivityLookup(); await credit();
  expect(events()).toHaveLength(1); expect(events()[0].kind).toBe("lookup");
  expect(vi.mocked(invoke).mock.calls.some(([, input]) => (input as { args: { action: string } }).args.action.startsWith("history"))).toBe(false);
  expect(invoke).toHaveBeenLastCalledWith("mobile_storage", { args: expect.objectContaining({ token: "generation", action: "activityRecord" }) });
});
it("credits only a changed forward page after dwell and invalidates reflow or stale navigation", async () => {
  const doc=document.implementation.createHTMLDocument();doc.body.innerHTML="<p>一二三四五六七八九十</p>";
  const node=doc.querySelector("p")!.firstChild!;let moved=false;
  const range=(offset:number)=>{const r=doc.createRange();r.setStart(node,offset);r.collapse(true);return r;};
  const view={getContents:()=>[],on:vi.fn(),off:vi.fn(),currentLocation:()=>({start:{index:0,cfi:moved?"next":"from"},end:{index:0,cfi:moved?"end":"to"}}),getRange:(cfi:string)=>range(({from:0,to:5,next:5,end:10} as Record<string,number>)[cfi])} as unknown as Rendition;
  const book={name:"fixture",bytes:new ArrayBuffer(0),id:"a".repeat(64)};
  const tracker=trackAndroidReading(view,book,false,vi.fn());await Promise.resolve();await Promise.resolve();
  const early=tracker.prepare();moved=true;early();await Promise.resolve();expect(events().filter(e=>e.kind==="characters")).toHaveLength(0);
  moved=false;await advance(4000);const unchanged=tracker.prepare();unchanged();await Promise.resolve();expect(events().filter(e=>e.kind==="characters")).toHaveLength(0);
  const stale=tracker.prepare();tracker.invalidate();moved=true;stale();await Promise.resolve();expect(events().filter(e=>e.kind==="characters")).toHaveLength(0);
  moved=false;await advance(4000);const commit=tracker.prepare();moved=true;commit();await advance(1);
  expect(events().filter(e=>e.kind==="characters")).toEqual([expect.objectContaining({startOffset:0,endOffset:5,spine:0})]);tracker.stop();
  moved=false;const fixed=trackAndroidReading(view,book,true,vi.fn());await Promise.resolve();await Promise.resolve();await advance(4000);const image=fixed.prepare();moved=true;image();await advance(1);expect(events().filter(e=>e.kind==="characters")).toHaveLength(1);fixed.stop();
});
