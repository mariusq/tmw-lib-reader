import { localDay } from "../../../packages/reader-core/readingActivity";
import { cleanup, fireEvent, render, screen, waitFor, act } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { EpubReader } from "./EpubReader";

const mock = vi.hoisted(() => ({
  listeners: new Map<string, (...args: unknown[]) => void>(),
  display: vi.fn(async () => {}),
  next: vi.fn(async () => {}),
  prev: vi.fn(async () => {}),
  contents: [] as Array<{ document: Document; cfiFromRange: (range: Range) => string }>,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => path,
}));
vi.mock("epubjs", () => ({
  default: () => ({
    opened: Promise.resolve(),
    packaging: { metadata: {} },
    spine: { hooks: { content: { register: vi.fn() } }, spineItems: [{ index: 0, linear: "yes" }], get: () => null },
    destroy: vi.fn(),
    renderTo: () => ({
      display: mock.display,
      next: mock.next,
      prev: mock.prev,
      currentLocation: () => ({}),
      on: (name: string, fn: (...args: unknown[]) => void) => {
        const previous = mock.listeners.get(name);
        mock.listeners.set(name, (...args) => { previous?.(...args); fn(...args); });
      },
      getContents: () => mock.contents,
      destroy: vi.fn(),
      themes: { default: vi.fn(), fontSize: vi.fn() },
    }),
  }),
}));
beforeEach(() => {
  mock.listeners.clear();
  mock.display.mockReset();
  mock.next.mockReset();
  mock.prev.mockReset();
  localStorage.clear();
  mock.contents = [];
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_reader_book") return { id: 1, filePath: "read-only.epub", title: "物語" };
    if (command === "get_reader_position") return { locationCfi: null, token: "initial", accepted: true };
    if (command === "save_reader_position") return { locationCfi: null, token: "saved", accepted: true };
    if (command === "lookup_reader_text" || command === "lookup_dictionary")
      return {
        engineVersion: 1,
        target: { surface: "読んだ", lemma: "読む", reading: "よむ" },
        matchedSpan: { start: 2, end: 5 },
        groups: [{ term: "読む", reading: "よむ", matches: [{
          matchedSpan: { start: 2, end: 5 }, deinflectionDepth: 1,
          entry: { id: 1, term: "読む", reading: "よむ", glossary: ["to read"],
            rules: [], definitionTags: [], termTags: [], score: 0, sequence: 1, priority: 0,
            provenance: { source: "yomitan:JMdict", title: "JMdict", revision: "fixture" } },
        }] }],
      };
    if (command === "list_dictionaries") return [];
    return false;
  });
});
afterEach(cleanup);

it("does not write the rounded opening position or write again on close", async () => {
  mock.display.mockImplementationOnce(async () => {
    mock.listeners.get("relocated")?.({ start: { cfi: "epubcfi(/6/2)" } });
  });
  const { unmount } = render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  unmount();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_reader_position" || command === "save_reading_location")).toBe(false);
});

it("follows the synced position on foreground refresh without writing when unchanged locally", async () => {
  let remote = false;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === "get_reader_book") return { id: 1, filePath: "fixture.epub", title: "Fixture" };
    if (command === "get_reader_position") return { locationCfi: remote ? "epubcfi(/6/80)" : "epubcfi(/6/20)", token: remote ? "phone" : "initial", accepted: true };
    return false;
  });
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  remote = true;
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(mock.display).toHaveBeenCalledWith("epubcfi(/6/80)");
  expect(screen.queryByText("Reading position updated on another device.")).toBeNull();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_reader_position")).toBe(false);
});

it("ignores layout relocations and saves a deliberate backwards turn", async () => {
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  const relocate = (cfi: string) => mock.listeners.get("relocated")?.({ start: { cfi } });
  const writes = () => vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_reader_position");
  await act(async () => {
    window.dispatchEvent(new Event("resize"));
    relocate("resize-cfi");
    fireEvent.click(screen.getByLabelText("Increase font size"));
    mock.listeners.get("rendered")?.();
    relocate("reflow-cfi");
  });
  expect(writes()).toHaveLength(0);
  mock.prev.mockImplementationOnce(async () => { relocate("earlier-cfi"); });
  fireEvent.click(screen.getByLabelText("Previous page"));
  await waitFor(() => expect(writes()).toHaveLength(1));
  expect(writes()[0][1]).toEqual({ bookId: 1, locationCfi: "earlier-cfi", expectedToken: "initial" });
  await act(async () => { relocate("another-layout-cfi"); });
  expect(writes()).toHaveLength(1);
});

it("preserves local navigation across re-pagination and offers an explicit synced jump", async () => {
  let remote = false;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_reader_book") return { id: 1, filePath: "fixture.epub", title: "Fixture" };
    if (command === "get_reader_position") return { locationCfi: remote ? "phone-cfi" : "old-cfi", token: remote ? "phone" : "initial", accepted: true };
    if (command === "save_reader_position") return { locationCfi: (args as { locationCfi: string }).locationCfi, token: "local", accepted: true };
    return false;
  });
  mock.next.mockImplementationOnce(async () => {
    mock.listeners.get("relocated")?.({ start: { cfi: "local-cfi" } });
  });
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  fireEvent.click(screen.getByLabelText("Next page"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_reader_position", { bookId: 1, locationCfi: "local-cfi", expectedToken: "initial" }));
  fireEvent.click(screen.getByLabelText("Hide furigana"));
  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_reader_open")).toHaveLength(2));
  remote = true;
  await act(async () => { window.dispatchEvent(new Event("focus")); });
  expect(screen.getByText("Reading position updated on another device.")).toBeTruthy();
  expect(mock.display).not.toHaveBeenCalledWith("phone-cfi");
  await act(async () => { fireEvent.click(screen.getByText("Go to synced position")); });
  expect(mock.display).toHaveBeenCalledWith("phone-cfi");
  expect(screen.queryByText("Reading position updated on another device.")).toBeNull();
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_reader_position")).toHaveLength(1);
});

it("saves the clicked ruby-free sentence and precise CFI in one action", async () => {
  const source = document.implementation.createHTMLDocument();
  source.body.innerHTML = "<p>前。<ruby>本<rt>ほん</rt></ruby>を読んだ。</p>";
  const node = source.querySelector("p")!.lastChild as Text;
  const range = source.createRange();
  range.setStart(node, 2);
  Object.defineProperty(source, "caretRangeFromPoint", { value: () => range });
  const anchor = "epubcfi(/6/2!/4/2/3:2)";
  mock.contents = [{ document: source, cfiFromRange: () => anchor }];
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  await act(async () => {
    mock.listeners.get("rendered")?.();
    source
      .querySelector("p")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, altKey: true }));
  });
  const save = await screen.findByText("Save passage");
  await screen.findByText("to read");
  expect(invoke).toHaveBeenCalledWith("lookup_reader_text", {
    request: { text: "前。本を読んだ。", offset: 5 },
  });
  fireEvent.click(save);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_passage", {
      request: {
        bookId: 1,
        surface: "読んだ",
        headword: "読む",
        reading: "よむ",
        sentence: "本を読んだ。",
        note: "",
        locationCfi: anchor,
      },
    }),
  );
  expect(await screen.findByRole("status")).toHaveTextContent("Passage saved");
  fireEvent.pointerDown(screen.getByLabelText("Offline dictionary"));
  expect(screen.getByLabelText("Offline dictionary")).toBeInTheDocument();
  act(() => { source.body.dispatchEvent(new Event("pointerdown", { bubbles: true })); });
  expect(screen.queryByLabelText("Offline dictionary")).not.toBeInTheDocument();
});

it("opens a bookmark anchor instead of the normal resume position", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_reader_book"
      ? { id: 1, filePath: "book.epub", title: "Book" }
      : command === "get_reader_position"
        ? { locationCfi: "resume-cfi", token: "initial", accepted: true }
        : command === "list_dictionaries"
          ? []
          : false,
  );
  render(<EpubReader bookId={1} initialCfi="bookmark-cfi" onClose={vi.fn()} />);
  await waitFor(() => expect(mock.display).toHaveBeenCalledWith("bookmark-cfi"));
});

it("records one encounter with portable dictionary identity and leaves manual queries unanchored", async () => {
  const source = document.implementation.createHTMLDocument();
  source.body.innerHTML = "<p>本を読んだ。</p>";
  const node = source.querySelector("p")!.firstChild as Text;
  const range = source.createRange();
  range.setStart(node, 3);
  Object.defineProperty(source, "caretRangeFromPoint", { value: () => range });
  const cfi = "epubcfi(/6/2!/4/2/1:3)";
  mock.contents = [{ document: source, cfiFromRange: () => cfi }];
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args, options) => {
    if (command === "record_lookup_history") return 2;
    return original(command, args, options);
  });
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  await act(async () => {
    mock.listeners.get("rendered")?.();
    source
      .querySelector("p")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true, altKey: true }));
  });
  expect(await screen.findByText(/Looked up 2 times/)).toBeVisible();
  const records = () =>
    vi.mocked(invoke).mock.calls.filter(([command]) => command === "record_lookup_history");
  expect(records()).toHaveLength(1);
  expect(records()[0][1]).toEqual({
    request: {
      activityDay: localDay(new Date()),
      surface: "読んだ",
      entryId: null, headword: "読む", reading: "よむ", dictionaryId: "yomitan:JMdict:fixture",
      bookId: 1,
      locationCfi: cfi,
      sentence: "本を読んだ。",
    },
  });
  // Query is still the surface form; retry the editable dictionary with a headword.
  const input = screen.getByDisplayValue("読んだ");
  fireEvent.change(input, { target: { value: "読む" } });
  fireEvent.click(screen.getByText("Look up"));
  await waitFor(() => expect(records()).toHaveLength(2));
  expect(records()[1][1]).toEqual({
    request: { activityDay: localDay(new Date()), surface: "読む", entryId: null, headword: "読む", reading: "よむ", dictionaryId: "yomitan:JMdict:fixture", bookId: null, locationCfi: "", sentence: "" },
  });
  expect(screen.queryByText("Save passage")).not.toBeInTheDocument();
});

it("shows the import prompt without fallback or history when no source is enabled", async () => {
  const source = document.implementation.createHTMLDocument();
  source.body.innerHTML = "<p>本を読んだ。</p>";
  const range = source.createRange();
  range.setStart(source.querySelector("p")!.firstChild!, 3);
  Object.defineProperty(source, "caretRangeFromPoint", { value: () => range });
  mock.contents = [{ document: source, cfiFromRange: () => "epubcfi(/6/2!/4/2/1:3)" }];
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args, options) => {
    if (command === "lookup_reader_text") throw new Error("Import a local Yomitan dictionary ZIP or enable a dictionary in Manage dictionaries.");
    return original(command, args, options);
  });
  render(<EpubReader bookId={1} onClose={vi.fn()} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("record_reader_open", { bookId: 1 }));
  await act(async () => {
    mock.listeners.get("rendered")?.();
    source.querySelector("p")!.dispatchEvent(new MouseEvent("click", { bubbles: true, altKey: true }));
  });
  expect(await screen.findByText(/Import a local Yomitan dictionary ZIP/)).toBeVisible();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "lookup_dictionary" || command === "record_lookup_history" || command === "ensure_bundled_dictionary")).toBe(false);
});
