import { cleanup, fireEvent, render, screen, waitFor, act } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { EpubReader } from "./EpubReader";

const mock = vi.hoisted(() => ({
  listeners: new Map<string, (...args: unknown[]) => void>(),
  display: vi.fn(async () => {}),
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
    spine: { spineItems: [{ index: 0, linear: "yes" }], get: () => null },
    destroy: vi.fn(),
    renderTo: () => ({
      display: mock.display,
      on: (name: string, fn: (...args: unknown[]) => void) => mock.listeners.set(name, fn),
      getContents: () => mock.contents,
      destroy: vi.fn(),
      themes: { default: vi.fn(), fontSize: vi.fn() },
    }),
  }),
}));
beforeEach(() => {
  mock.listeners.clear();
  mock.display.mockClear();
  mock.contents = [];
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_reader_book") return { id: 1, filePath: "read-only.epub", title: "物語" };
    if (command === "get_reading_location") return null;
    if (command === "tokenize_dictionary_target")
      return { surface: "読んだ", lemma: "読む", reading: "よむ" };
    if (command === "lookup_dictionary")
      return [
        {
          id: 1,
          term: "読む",
          reading: "よむ",
          definitions: ["to read"],
          partOfSpeech: [],
          dictionaryName: "JMdict",
        },
      ];
    if (command === "list_dictionaries") return [];
    return false;
  });
});
afterEach(cleanup);

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
});

it("opens a bookmark anchor instead of the normal resume position", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_reader_book"
      ? { id: 1, filePath: "book.epub", title: "Book" }
      : command === "get_reading_location"
        ? "resume-cfi"
        : command === "list_dictionaries"
          ? []
          : false,
  );
  render(<EpubReader bookId={1} initialCfi="bookmark-cfi" onClose={vi.fn()} />);
  await waitFor(() => expect(mock.display).toHaveBeenCalledWith("bookmark-cfi"));
});

it("records one encounter after dictionary fallbacks and leaves manual queries unanchored", async () => {
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
    if (command === "lookup_dictionary" && (args as { query: string }).query === "読んだ")
      return [];
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
      surface: "読んだ",
      entryId: 1,
      bookId: 1,
      locationCfi: cfi,
      sentence: "本を読んだ。",
    },
  });
  fireEvent.click(screen.getByText("Look up"));
  // Query is still the surface form; retry the editable dictionary with a headword.
  const input = screen.getByDisplayValue("読んだ");
  fireEvent.change(input, { target: { value: "読む" } });
  fireEvent.click(screen.getByText("Look up"));
  await waitFor(() => expect(records()).toHaveLength(2));
  expect(records()[1][1]).toEqual({
    request: { surface: "読む", entryId: 1, bookId: null, locationCfi: "", sentence: "" },
  });
  expect(screen.queryByText("Save passage")).not.toBeInTheDocument();
});
