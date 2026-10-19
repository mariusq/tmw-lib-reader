import { act, cleanup, render, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useRef } from "react";
import type { Rendition } from "epubjs";
import { invoke } from "@tauri-apps/api/core";
import { useReadingActivity } from "./useReadingActivity";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
const native = vi.hoisted(() => ({ listener: null as null | ((event: {payload: boolean}) => void), unlisten: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ isFocused: async () => true, onFocusChanged: async (listener: (event: {payload:boolean}) => void) => { native.listener = listener; return native.unlisten; } }) }));
let mono = 0;
const error = vi.fn();
function Harness() {
  const view = useRef({ getContents: () => [], on: vi.fn(), off: vi.fn() } as unknown as Rendition);
  useReadingActivity(1, true, view, error);
  return null;
}
async function advance(ms: number) {
  await act(async () => {});
  mono += ms;
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 9, 13, 12));
  mono = 0;
  vi.spyOn(performance, "now").mockImplementation(() => mono);
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  vi.mocked(invoke).mockClear();
  error.mockClear();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("checkpoints focused time and flushes a short session on close", async () => {
  const view = render(<Harness />);
  await advance(10000);
  expect(invoke).toHaveBeenCalledWith("record_reading_activity", {
    request: expect.objectContaining({ bookId: 1, milliseconds: 10000, day: "2026-10-13" }),
  });
  mono += 2000;
  vi.setSystemTime(Date.now() + 2000);
  view.unmount();
  await act(async () => {});
  expect(invoke).toHaveBeenLastCalledWith("record_reading_activity", {
    request: expect.objectContaining({ milliseconds: 2000 }),
  });
});
it("does not count background or sleeping time", async () => {
  render(<Harness />);
  await advance(10000);
  act(() => native.listener?.({payload:false}));
  vi.mocked(invoke).mockClear();
  await advance(10000);
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_reading_activity")).toHaveLength(0);
  act(() => native.listener?.({payload:true}));
  await advance(60000);
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_reading_activity")).toHaveLength(0);
});
it("stops after the idle cutoff and resumes after intentional input", async () => {
  render(<Harness />);
  for (let i = 0; i < 180; i++) await advance(10000);
  vi.mocked(invoke).mockClear();
  await advance(10000);
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_reading_activity")).toHaveLength(0);
  act(() => document.dispatchEvent(new Event("pointerdown")));
  await advance(10000);
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_reading_activity")).toHaveLength(1);
});

it("counts only a deliberate changed page after focused dwell", async () => {
  const doc = document.implementation.createHTMLDocument();
  doc.body.innerHTML = "<p>本を読む。</p>";
  const from = doc.createRange(),
    to = doc.createRange();
  const node = doc.querySelector("p")!.firstChild!;
  from.setStart(node, 0);
  from.collapse(true);
  to.setStart(node, 5);
  to.collapse(true);
  let moved = false;
  const view = {
    getContents: () => [],
    on: vi.fn(),
    off: vi.fn(),
    getRange: (cfi: string) => (cfi === "from" ? from : to),
    currentLocation: () => ({
      start: { index: 0, cfi: moved ? "next" : "from", displayed: { page: 1, total: 2 } },
      end: { index: 0, cfi: "to", displayed: { page: 1, total: 2 } },
    }),
  } as unknown as Rendition;
  function CharacterHarness() {
    const ref = useRef(view);
    const prepare = useReadingActivity(1, true, ref, error, "fixture");
    return (
      <button
        onClick={() => {
          const commit = prepare.current();
          moved = true;
          commit();
        }}
      >
        Next
      </button>
    );
  }
  const rendered = render(<CharacterHarness />);
  fireEvent.click(screen.getByText("Next"));
  await act(async () => {});
  expect(invoke).not.toHaveBeenCalledWith("record_reading_characters", expect.anything());
  rendered.unmount();
  moved = false;
  render(<CharacterHarness />);
  await advance(10000);
  fireEvent.click(screen.getByText("Next"));
  await act(async () => {});
  expect(invoke).toHaveBeenCalledWith("record_reading_characters", {
    request: {
      bookId: 1,
      contentKey: "fixture",
      day: "2026-10-13",
      spine: 0,
      startOffset: 0,
      endOffset: 5,
    },
  });
});

it("counts a ten-minute page without any input", async () => {
 render(<Harness />);
 for (let i=0;i<60;i++) await advance(10000);
 const checkpoints = vi.mocked(invoke).mock.calls.filter(([command])=>command === "record_reading_activity");
 expect(checkpoints.reduce((sum,[,args])=>sum+(args as {request:{milliseconds:number}}).request.milliseconds,0)).toBe(600000);
});

it("continues counting when the DOM loses focus to the EPUB iframe", async () => {
 const rendered = render(<Harness />); await act(async () => {});
 vi.mocked(document.hasFocus).mockReturnValue(false);
 act(() => window.dispatchEvent(new Event("blur")));
 for (let i=0;i<60;i++) await advance(10000);
 const checkpoints=vi.mocked(invoke).mock.calls.filter(([command])=>command === "record_reading_activity");
 expect(checkpoints.reduce((sum,[,args])=>sum+(args as {request:{milliseconds:number}}).request.milliseconds,0)).toBe(600000);
 rendered.unmount();expect(native.unlisten).toHaveBeenCalled();
});
