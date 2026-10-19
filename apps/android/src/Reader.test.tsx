import { act, render, fireEvent, screen, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import Reader from "./Reader";

const mock = vi.hoisted(() => ({
  display: vi.fn(async (_cfi?: string) => {}),
  next: vi.fn(async () => {}),
  prev: vi.fn(async () => {}),
  direction: vi.fn(),
  resize: vi.fn(),
  currentCfi: undefined as string | undefined,
  relocated: undefined as undefined | ((location: { start: { cfi: string } }) => void),
  destroy: vi.fn(),
  content: undefined as
    undefined | ((contents: { document: Document; cfiFromRange: () => string }) => void),
  caret: undefined as undefined | { node: Text; offset: number },
}));
vi.mock("../../../packages/reader-core/caret", () => ({ caretAt: () => mock.caret }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./localBook", () => ({ localBook: vi.fn(async () => undefined) }));
vi.mock("epubjs", () => ({
  default: () => ({
    opened: Promise.resolve(),
    destroy: mock.destroy,
    renderTo: () => ({
      hooks: {
        content: {
          register: (hook: typeof mock.content) => {
            mock.content = hook;
          },
        },
      },
      display: mock.display,
      started: Promise.resolve(),
      direction: mock.direction,
      next: mock.next,
      prev: mock.prev,
      resize: mock.resize,
      currentLocation: () => mock.currentCfi ? { start: { cfi: mock.currentCfi } } : undefined,
      on: (_event: string, handler: typeof mock.relocated) => {
        if (_event === "relocated" && !mock.relocated) mock.relocated = handler;
      },
      destroy: () => { mock.relocated = undefined; },
    }),
  }),
}));
beforeEach(() => {
  vi.useFakeTimers();
  mock.display.mockImplementation(async () => {});
  mock.next.mockImplementation(async () => {});
  mock.prev.mockImplementation(async () => {});
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.mocked(invoke).mockClear();
  localStorage.clear();
  mock.display.mockReset();
  mock.next.mockReset();
  mock.prev.mockReset();
  mock.currentCfi = undefined;
  mock.relocated = undefined;
});
function relocate(cfi: string) {
  mock.currentCfi = cfi;
  mock.relocated?.({ start: { cfi } });
}
async function finishNavigation() {
  await act(async () => { await vi.advanceTimersByTimeAsync(80); });
}
const progressSaves = () => vi.mocked(invoke).mock.calls.filter(([, input]) => {
  const args = (input as { args?: { action?: string; kind?: string } })?.args;
  return args?.action === "userSave" && args.kind === "progress";
});
it("a completed ordinary word tap shows definitions and records history without saving an excerpt", async () => {
  vi.mocked(invoke).mockImplementation(async (command, input) => {
    if (command === "lookup_text")
      return {
        target: { surface: "猫", lemma: "猫", reading: "ねこ" },
        entries: [{ term: "猫", reading: "ねこ", definitions: ["cat"] }],
        elapsedMs: 1,
        dictionaryBytes: 1,
      };
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "userState") return { progress: null, passages: [], next: null };
    return { recorded: true };
  });
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  await act(async () => {
    render(<Reader selected={selected} />);
  });
  const doc = document.implementation.createHTMLDocument();
  const p = doc.createElement("p");
  p.textContent = "猫。";
  doc.body.append(p);
  mock.caret = { node: p.firstChild as Text, offset: 0 };
  mock.content?.({ document: doc, cfiFromRange: () => "epubcfi(/6/2)" });
  function pointer(type: string) {
    const event = new Event(type, { bubbles: true });
    Object.assign(event, { isPrimary: true, clientX: 0, clientY: 0, pointerId: 1 });
    p.dispatchEvent(event);
  }
  await act(async () => {
    pointer("pointerdown");
    pointer("pointerup");
  });
  expect(invoke).toHaveBeenCalledWith("mobile_storage", {
    args: expect.objectContaining({
      action: "historyRecord",
      ...identity,
      fields: expect.objectContaining({ surface: "猫", sentence: "猫。" }),
    }),
  });
  expect(invoke).not.toHaveBeenCalledWith("mobile_storage", {
    args: expect.objectContaining({ action: "userSave", kind: "passage" }),
  });
  const popup = screen.getByRole("dialog", { name: "猫" });
  // The opening touch's synthesized click lands outside the newly opened modal.
  fireEvent.click(popup, { clientX: -1, clientY: -1 });
  expect(popup.hasAttribute("open")).toBe(true);
  const down = new Event("pointerdown", { bubbles: true });
  Object.assign(down, { clientX: -1, clientY: -1 });
  fireEvent(popup, down);
  fireEvent.click(popup, { clientX: -1, clientY: -1 });
  expect(popup.hasAttribute("open")).toBe(false);
});
const identity = { ns: "a".repeat(32), id: "b".repeat(32), version: `sha256-${"c".repeat(64)}` };
const selected = { name: "Book", id: "c".repeat(64), bytes: new ArrayBuffer(0), catalog: identity };
const nativeState = (cfi: string) => ({
  progress: { contentVersion: identity.version, deleted: false, fields: { locationCfi: cfi } },
  passages: [], pending: 0, next: null,
});

it("reanchors reflow and pause/resume cleanup without saving rounded page starts", async () => {
  vi.mocked(invoke).mockResolvedValue(nativeState("epubcfi(/6/40)"));
  mock.display.mockImplementationOnce(async () => { relocate("epubcfi(/6/38)"); });
  let unmount!: () => void;
  await act(async () => { ({ unmount } = render(<Reader selected={selected} />)); });
  const pane = screen.getByLabelText("Book pages");
  vi.spyOn(pane, "getBoundingClientRect").mockReturnValue({ width: 400, height: 800 } as DOMRect);
  mock.display.mockImplementationOnce(async () => { relocate("epubcfi(/6/36)"); });
  await act(async () => {
    window.dispatchEvent(new Event("resize"));
    await vi.advanceTimersByTimeAsync(200);
  });
  expect(mock.resize).toHaveBeenCalledWith(400, 800);
  expect(mock.display).toHaveBeenLastCalledWith("epubcfi(/6/38)");
  await act(async () => {
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
    window.dispatchEvent(new Event("pagehide"));
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "resume" }));
  });
  expect(mock.display).toHaveBeenLastCalledWith("epubcfi(/6/40)");
  await act(async () => { unmount(); });
  expect(progressSaves()).toHaveLength(0);
});

it("periodically follows a PC change while open without writing its rounded display", async () => {
  let state = nativeState("epubcfi(/6/2)");
  vi.mocked(invoke).mockImplementation(async (_, input) =>
    (input as { args?: { action?: string } })?.args?.action === "historyList" ? undefined : state);
  mock.display.mockImplementation(async cfi => { if (cfi) relocate(cfi); });
  await act(async () => { render(<Reader selected={selected} />); });
  state = nativeState("epubcfi(/6/80)");
  mock.display.mockImplementationOnce(async () => { relocate("epubcfi(/6/78)"); });
  await act(async () => { await vi.advanceTimersByTimeAsync(10000); });
  expect(mock.display).toHaveBeenLastCalledWith("epubcfi(/6/80)");
  expect(screen.getByText("Moved to synced reading position.")).toBeTruthy();
  expect(screen.queryByText("Reading position updated on another device.")).toBeNull();
  // The canonical CFI remains the agreement, even when epub.js rounded it.
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(mock.display).toHaveBeenCalledTimes(2);
  expect(progressSaves()).toHaveLength(0);
});

it.each(["Keep this position", "Go to synced position"])(
  "holds dirty navigation through pause/resume and further turns until %s",
  async choice => {
    let complete!: (state: ReturnType<typeof nativeState>) => void;
    let prepared: ReturnType<typeof nativeState> | Promise<ReturnType<typeof nativeState>> = nativeState("epubcfi(/6/2)");
    vi.mocked(invoke).mockImplementation(async (_, input) => {
      const args = (input as { args: Record<string, unknown> }).args;
      return args.action === "userPrepareOpen" ? prepared : nativeState("epubcfi(/6/2)");
    });
    mock.display.mockImplementation(async cfi => { if (cfi) relocate(cfi); });
    await act(async () => { render(<Reader selected={selected} />); });
    prepared = new Promise(resolve => { complete = resolve; });
    // A turn made while a PC check is in flight is held until the check answers.
    await act(async () => { window.dispatchEvent(new Event("focus")); });
    mock.next.mockImplementationOnce(async () => { relocate("epubcfi(/6/4)"); });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Next page" })); });
    await finishNavigation();
    expect(progressSaves()).toHaveLength(0);
    await act(async () => {
      prepared = nativeState("epubcfi(/6/80)");
      complete(prepared);
    });
    expect(screen.getByText("Reading position updated on another device.")).toBeTruthy();
    await act(async () => {
      window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
      window.dispatchEvent(new Event("pagehide"));
      window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "resume" }));
    });
    expect(mock.display).toHaveBeenLastCalledWith("epubcfi(/6/4)");
    expect(screen.getByText("Reading position updated on another device.")).toBeTruthy();
    mock.next.mockImplementationOnce(async () => { relocate("epubcfi(/6/6)"); });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Next page" })); });
    await finishNavigation();
    expect(progressSaves()).toHaveLength(0);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: choice })); });
    expect(screen.queryByText("Reading position updated on another device.")).toBeNull();
    if (choice === "Keep this position") {
      expect(progressSaves()).toHaveLength(1);
      expect(progressSaves()[0][1]).toEqual({ args: expect.objectContaining({
        kind: "progress", fields: { locationCfi: "epubcfi(/6/6)" },
      }) });
    } else {
      expect(mock.display).toHaveBeenLastCalledWith("epubcfi(/6/80)");
      await act(async () => { window.dispatchEvent(new Event("pagehide")); });
      expect(progressSaves()).toHaveLength(0);
    }
  },
);

it.each(["deleted", "version"])("ignores %s PC progress during visible checks", async invalid => {
  let state = nativeState("epubcfi(/6/2)");
  vi.mocked(invoke).mockImplementation(async () => state);
  await act(async () => { render(<Reader selected={selected} />); });
  state = nativeState("epubcfi(/6/80)");
  if (invalid === "deleted") state.progress.deleted = true;
  else state.progress.contentVersion = "other-version";
  await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
  expect(mock.display).toHaveBeenCalledTimes(1);
  expect(progressSaves()).toHaveLength(0);
});

it("does not overlap visible checks, skips hidden checks, and quietly saves navigation after an offline check", async () => {
  let fail!: (reason: Error) => void;
  let prepared: ReturnType<typeof nativeState> | Promise<ReturnType<typeof nativeState>> = nativeState("epubcfi(/6/2)");
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "historyList") return undefined;
    return args.action === "userPrepareOpen" ? prepared : nativeState("epubcfi(/6/2)");
  });
  const prepareCalls = () => vi.mocked(invoke).mock.calls.filter(([, input]) =>
    (input as { args?: { action?: string } })?.args?.action === "userPrepareOpen");
  await act(async () => { render(<Reader selected={selected} />); });
  prepared = new Promise((_, reject) => { fail = reject; });
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  mock.next.mockImplementationOnce(async () => { relocate("epubcfi(/6/4)"); });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Next page" })); });
  await finishNavigation();
  await act(async () => { await vi.advanceTimersByTimeAsync(20000); });
  expect(prepareCalls()).toHaveLength(2);
  expect(progressSaves()).toHaveLength(0);
  await act(async () => { fail(new Error("PC unavailable")); });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(progressSaves()).toHaveLength(1);
  const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  await act(async () => { await vi.advanceTimersByTimeAsync(10000); });
  expect(prepareCalls()).toHaveLength(2);
  visibility.mockRestore();
});

it("offers manual offline lookup in the reader menu without borrowing book context", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "lookup_text") return { target: { surface: "猫", lemma: "猫", reading: "ねこ" }, entries: [{ term: "猫", reading: "ねこ", definitions: ["cat"] }], elapsedMs: 1, dictionaryBytes: 1 };
    if (command === "dictionary_manage") return [];
    if (command === "dictionary_import_status") return { running: false, progress: {} };
    return { progress: null, passages: [], next: null };
  });
  await act(async () => { render(<Reader />); });
  fireEvent.click(screen.getByRole("button", { name: "Open reader menu" }));
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Dictionary" })); });
  fireEvent.change(screen.getByLabelText("Japanese word"), { target: { value: "猫" } });
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Look up" })); });
  expect(invoke).toHaveBeenCalledWith("lookup_text", { text: "猫", offset: 0 });
  expect(screen.getByText("cat")).toBeTruthy();
});
it("restores native progress and saves a deliberate turn once, including backwards reading", async () => {
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    return args.action === "userState" || args.action === "userPrepareOpen"
      ? {
          progress: { contentVersion: identity.version, fields: { locationCfi: "epubcfi(/6/2)" } },
          passages: [],
          next: null,
        }
      : {};
  });
  await act(async () => {
    render(<Reader selected={selected} />);
  });
  expect(mock.display).toHaveBeenCalledWith("epubcfi(/6/2)");
  mock.next.mockImplementationOnce(async () => { relocate("epubcfi(/6/4)"); });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  });
  // Pause before the animation-frame settlement: a confirmed deliberate
  // relocation still checkpoints once, without waiting for another page turn.
  await act(async () => {
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
  });
  await finishNavigation();
  expect(invoke).toHaveBeenCalledWith("mobile_storage", {
    args: expect.objectContaining({
      action: "userSave",
      ...identity,
      kind: "progress",
      fields: { locationCfi: "epubcfi(/6/4)" },
    }),
  });
  expect(localStorage.getItem(`tmw-local-cfi-${selected.id}`)).toBe("epubcfi(/6/4)");
  expect(progressSaves()).toHaveLength(1);
  await act(async () => {
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
    window.dispatchEvent(new Event("pagehide"));
  });
  expect(progressSaves()).toHaveLength(1);
  mock.prev.mockImplementationOnce(async () => { relocate("epubcfi(/6/2)"); });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  });
  await finishNavigation();
  expect(localStorage.getItem(`tmw-local-cfi-${selected.id}`)).toBe("epubcfi(/6/2)");
  expect(progressSaves()).toHaveLength(2);
});
it("does not restore an anchor from a different source version", async () => {
  vi.mocked(invoke).mockResolvedValue({
    progress: {
      contentVersion: `sha256-${"d".repeat(64)}`,
      fields: { locationCfi: "epubcfi(/6/old)" },
    },
    passages: [],
    next: null,
  });
  await act(async () => {
    render(<Reader selected={selected} />);
  });
  expect(mock.display).toHaveBeenCalledWith(undefined);
});

it("waits for PC progress before display and checks again on foreground resume", async () => {
  let complete!: (state: unknown) => void;
  const pending = new Promise(resolve => { complete = resolve; });
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "userPrepareOpen") return pending;
    return {};
  });
  await act(async () => { render(<Reader selected={selected} />); });
  expect(mock.display).not.toHaveBeenCalled();
  await act(async () => {
    complete({ progress: { contentVersion: identity.version, fields: { locationCfi: "epubcfi(/6/80)" } } });
  });
  expect(mock.display).toHaveBeenCalledWith("epubcfi(/6/80)");
  expect(invoke).not.toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "userSave" }) });
  const before = vi.mocked(invoke).mock.calls.filter(([, input]) => (input as { args?: { action?: string } })?.args?.action === "userPrepareOpen").length;
  await act(async () => { window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "resume" })); });
  expect(vi.mocked(invoke).mock.calls.filter(([, input]) => (input as { args?: { action?: string } })?.args?.action === "userPrepareOpen")).toHaveLength(before + 1);
});

it("opens offline from saved progress without uploading the initial rounded page", async () => {
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "userPrepareOpen") throw new Error("PC unavailable");
    if (args.action === "userState") return { progress: { contentVersion: identity.version, fields: { locationCfi: "epubcfi(/6/40)" } } };
    return {};
  });
  mock.display.mockImplementationOnce(async () => { mock.relocated?.({ start: { cfi: "epubcfi(/6/38)" } }); });
  let unmount!: () => void;
  await act(async () => { ({ unmount } = render(<Reader selected={selected} />)); });
  expect(mock.display).toHaveBeenCalledWith("epubcfi(/6/40)");
  expect(screen.getAllByText(/Could not check the PC reading position/).length).toBeGreaterThan(0);
  await act(async () => {
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
    unmount();
  });
  expect(invoke).not.toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "userSave" }) });
});

it("opens the immersive control sheet with a bottom-edge swipe and checkpoints before exit", async () => {
  vi.mocked(invoke).mockResolvedValue({ progress: null, passages: [], next: null });
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  const exit = vi.fn();
  await act(async () => {
    render(<Reader selected={selected} onExit={exit} />);
  });
  const handle = screen.getByRole("button", { name: "Open reader menu" });
  handle.setPointerCapture = vi.fn();
  const down = new Event("pointerdown", { bubbles: true });
  Object.assign(down, { clientX: 150, clientY: 800, pointerId: 1 });
  const up = new Event("pointerup", { bubbles: true });
  Object.assign(up, { clientX: 160, clientY: 730, pointerId: 1 });
  await act(async () => {
    handle.dispatchEvent(down);
    handle.dispatchEvent(up);
  });
  expect(screen.getByRole("dialog", { name: "Reading controls" }).getAttribute("open")).toBe("");
  fireEvent.click(screen.getByRole("button", { name: "Exit reader" }));
  expect(exit).toHaveBeenCalledOnce();
});

it("turns pages directly from the visible bar without opening the menu", async () => {
  vi.mocked(invoke).mockResolvedValue({ progress: null, passages: [], next: null });
  mock.next.mockClear();
  mock.prev.mockClear();
  await act(async () => {
    render(<Reader selected={selected} />);
  });
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  });
  await finishNavigation();
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
  });
  expect(mock.next).toHaveBeenCalledOnce();
  expect(mock.prev).toHaveBeenCalledOnce();
  expect(mock.direction).toHaveBeenCalledWith("ltr");
  expect(screen.getByText("Reading controls").closest("dialog")?.hasAttribute("open")).toBe(false);
});

it("turns blank edges in a translated chapter but ignores blank center taps and drags", async () => {
  vi.mocked(invoke).mockResolvedValue({ progress: null, passages: [], next: null });
  mock.next.mockClear();
  mock.prev.mockClear();
  mock.caret = undefined;
  vi.mocked(invoke).mockClear();
  await act(async () => {
    render(<Reader selected={selected} />);
  });
  const pane = screen.getByLabelText("Book pages");
  vi.spyOn(pane, "getBoundingClientRect").mockReturnValue({ left: 10, width: 390 } as DOMRect);
  const iframe = document.createElement("iframe");
  document.body.append(iframe);
  try {
    vi.spyOn(iframe, "getBoundingClientRect").mockReturnValue({ left: -770 } as DOMRect);
    const doc = iframe.contentDocument!;
    mock.content?.({ document: doc, cfiFromRange: () => "epubcfi(/6/2)" });
    function pointer(type: string, x: number) {
      const event = new Event(type, { bubbles: true });
      Object.assign(event, { isPrimary: true, clientX: x, clientY: 100, pointerId: 1 });
      doc.body.dispatchEvent(event);
    }
    async function tap(x: number) {
      await act(async () => {
        pointer("pointerdown", x);
        pointer("pointerup", x);
      });
      await finishNavigation();
    }
    await tap(1160); // 380px into the pane, although 1160px into the chapter.
    await tap(790); // 10px into the pane.
    await tap(970); // Center blank space.
    await act(async () => {
      pointer("pointerdown", 1160);
      pointer("pointermove", 1120);
      pointer("pointerup", 1160);
    });
    expect(mock.next).toHaveBeenCalledOnce();
    expect(mock.prev).toHaveBeenCalledOnce();
    expect(invoke).not.toHaveBeenCalledWith("lookup_text", expect.anything());
    expect(screen.getByText("Reading controls").closest("dialog")?.hasAttribute("open")).toBe(
      false,
    );
  } finally {
    iframe.remove();
  }
});


