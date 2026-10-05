import { act, render, fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import Reader from "./Reader";

const mock = vi.hoisted(() => ({
  display: vi.fn(async () => {}),
  next: vi.fn(async () => {}),
  prev: vi.fn(async () => {}),
  direction: vi.fn(),
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
      currentLocation: () => undefined,
      on: (_event: string, handler: typeof mock.relocated) => {
        mock.relocated = handler;
      },
      destroy: vi.fn(),
    }),
  }),
}));
afterEach(() => {
  localStorage.clear();
  mock.display.mockClear();
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
it("restores version-matched native progress and saves relocation to the durable queue", async () => {
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    return args.action === "userState"
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
  await act(async () => {
    mock.relocated?.({ start: { cfi: "epubcfi(/6/4)" } });
  });
  expect(invoke).toHaveBeenCalledWith("mobile_storage", {
    args: expect.objectContaining({
      action: "userSave",
      ...identity,
      kind: "progress",
      fields: { locationCfi: "epubcfi(/6/4)" },
    }),
  });
  expect(localStorage.getItem(`tmw-local-cfi-${selected.id}`)).toBe("epubcfi(/6/4)");
  const savesBeforePause = vi
    .mocked(invoke)
    .mock.calls.filter(
      ([, input]) => (input as { args?: { action?: string } })?.args?.action === "userSave",
    ).length;
  await act(async () => {
    window.dispatchEvent(new CustomEvent("tmw-lifecycle", { detail: "pause" }));
    window.dispatchEvent(new Event("pagehide"));
  });
  expect(
    vi
      .mocked(invoke)
      .mock.calls.filter(
        ([, input]) => (input as { args?: { action?: string } })?.args?.action === "userSave",
      ),
  ).toHaveLength(savesBeforePause);
  await act(async () => {
    mock.relocated?.({ start: { cfi: "epubcfi(/6/2)" } });
  });
  expect(localStorage.getItem(`tmw-local-cfi-${selected.id}`)).toBe("epubcfi(/6/2)");
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


