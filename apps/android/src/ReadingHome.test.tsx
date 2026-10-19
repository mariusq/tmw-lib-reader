import { act, fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import ReadingHome from "./ReadingHome";
import { localDay, weekStart } from "../../../packages/reader-core/readingActivity";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./Catalog", () => ({ Cover: () => <span>cover</span> }));
afterEach(() => { cleanup(); localStorage.clear(); });
it("shows compact statistics, selectable calendar, Resume and the full want shelf", async () => {
  const book = { ns: "a".repeat(32), id: "b".repeat(32), title: "Fixture", creator: "Author", download: { file: "fixture.epub", version: `sha256-${"c".repeat(64)}` } };
  vi.mocked(invoke).mockImplementation(async (_command, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "browse") return { ns: book.ns, items: [book] };
    if (args.action === "activitySummary") return { enabled: true, days: [], books: [], finished: 2, streak: 3, longestStreak: 8, syncMessage: "Activity synced" };
    return {};
  });
  const library = vi.fn();
  await act(async () => { render(<ReadingHome onOpen={vi.fn()} onLibrary={library} />); });
  expect(screen.getByText("Resume · Fixture")).toBeTruthy();
  expect(screen.getByText("Current streak")).toBeTruthy();
  expect(screen.getAllByRole("button", { pressed: false })).toHaveLength(6);
  expect(screen.getByLabelText("Next week").hasAttribute("disabled")).toBe(true);
  await act(async () => { fireEvent.click(screen.getByLabelText("Previous week")); });
  const previous = new Date(); previous.setDate(previous.getDate() - 7);
  expect(invoke).toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "activitySummary", selectedDay: localDay(previous) }) });
  expect(screen.getByLabelText("Next week").hasAttribute("disabled")).toBe(false);
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Today" })); });
  fireEvent.change(screen.getByLabelText("Heatmap metric"), { target: { value: "characters" } });
  expect(localStorage.getItem("tmw-home-metric")).toBe("characters");
  fireEvent.click(screen.getByText("View shelf")); expect(library).toHaveBeenCalledWith("want");
  expect(screen.getByText("Tracking & sync").parentElement?.hasAttribute("open")).toBe(false);
  fireEvent.click(screen.getByText("Tracking & sync"));
  fireEvent.click(screen.getByText("Clear activity"));
  expect(screen.getByRole("alertdialog").textContent).toContain("lookup history are kept");
  expect(vi.mocked(invoke).mock.calls.some(([,input]) => (input as {args:{action:string}}).args.action==="activityClear")).toBe(false);
  fireEvent.click(screen.getByText("Sync activity now"));
  expect(invoke).toHaveBeenCalledWith("mobile_storage", { args: { action: "activitySync" } });
});

it("keeps day selection and the entire history accessible through bounded week navigation", async () => {
  vi.mocked(invoke).mockImplementation(async (_command, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    if (args.action === "browse") return { ns: "fixture", items: [] };
    if (args.action === "activitySummary") return { enabled: false, days: [], books: [{ id: "book", title: "Selected book", milliseconds: 60000, characters: 20, lookups: 1 }], finished: 0, streak: 0, longestStreak: 0, syncMessage: "Activity saved locally", activitySync: { busy: false, pending: 2, rejected: 0 } };
    return {};
  });
  await act(async () => { render(<ReadingHome onOpen={vi.fn()} onLibrary={vi.fn()} />); });
  expect(screen.getByText("Tracking paused")).toBeTruthy();
  expect(screen.getByText("Tracking & sync · 2 pending")).toBeTruthy();
  const monday = localDay(weekStart(new Date()));
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: `${monday}: 0m` })); });
  expect(invoke).toHaveBeenCalledWith("mobile_storage", { args: expect.objectContaining({ action: "activitySummary", selectedDay: monday }) });
  const books = screen.getByText("Books · 1").parentElement;
  expect(books?.hasAttribute("open")).toBe(false);
  for (let index = 0; index < 13; index++) {
    await act(async () => { fireEvent.click(screen.getByLabelText("Previous week")); });
  }
  expect(screen.getByLabelText("Previous week").hasAttribute("disabled")).toBe(true);
  const oldest = new Date(); oldest.setDate(oldest.getDate() - 83);
  expect(screen.getByRole("button", { name: `${localDay(oldest)}: 0m` }).getAttribute("aria-pressed")).toBe("true");
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Today" })); });
  expect(screen.getByRole("button", { name: `${localDay(new Date())}: 0m` }).getAttribute("aria-pressed")).toBe("true");
});
