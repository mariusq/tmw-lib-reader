import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ReadingHome } from "./ReadingHome";
import { localDay } from "../../packages/reader-core/readingActivity";
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => path,
}));
vi.mock("./ContinueReading", () => ({ ContinueReading: () => <div>Resume shelf</div> }));
vi.mock("../features/reader/EpubReader", () => ({
  EpubReader: ({ bookId, onClose }: { bookId: number; onClose: () => void }) => (
    <div>
      Reader {bookId}
      <button onClick={onClose}>Close</button>
    </div>
  ),
}));
beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_reading_activity"
      ? {
          enabled: true,
          days: [{ day: localDay(new Date()), milliseconds: 1800000, characters: 1234, lookups: 7 }],
          books: [{ id: 1, title: "Story", milliseconds: 1800000, characters: 1234 }],
          finished: 2,
          streak: 3,
          longestStreak: 6,
        }
      : command === "browse_books"
        ? [
            {
              id: 1,
              effectiveTitle: "Next story",
              effectiveCreator: "Author",
              effectiveCoverPath: "cover.jpg",
              isAvailable: true,
            },
          ]
        : undefined,
  );
});
it("shows real weekly activity, accessible days and a cover-free next read", async () => {
  const shelf = vi.fn();
  render(
    <ReadingHome
      language="en"
      showCovers={false}
      onLibrary={vi.fn()}
      onRoots={vi.fn()}
      onWant={shelf}
    />,
  );
  expect(await screen.findByText("Story")).toBeInTheDocument();
  expect(screen.getByText("Books marked finished")).toBeInTheDocument();
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  const selected = screen
    .getAllByRole("button")
    .find((button) => button.getAttribute("aria-pressed") === "true")!;
  fireEvent.keyDown(selected, { key: "ArrowLeft" });
  await waitFor(() =>
    expect(invoke).toHaveBeenLastCalledWith(
      "get_reading_activity",
      expect.objectContaining({ selectedDay: expect.any(String) }),
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "View shelf →" }));
  expect(shelf).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "Open: Next story" }));
  expect(screen.getByText("Reader 1")).toBeInTheDocument();
});
it("requires a concrete confirmation before clearing only activity", async () => {
  render(
    <ReadingHome
      language="en"
      showCovers={true}
      onLibrary={vi.fn()}
      onRoots={vi.fn()}
      onWant={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Clear activity" }));
  expect(invoke).not.toHaveBeenCalledWith("clear_reading_activity");
  fireEvent.click(screen.getByRole("button", { name: "Clear reading activity" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_reading_activity"));
});

it("shows lookup and character totals, streaks, and switches the heatmap metric", async () => {
 render(<ReadingHome language="en" showCovers={false} onLibrary={vi.fn()} onRoots={vi.fn()} onWant={vi.fn()} />);
 await screen.findByText("Story");
 expect(screen.getByText("1,234")).toBeInTheDocument();expect(screen.getByText("7")).toBeInTheDocument();
 const streak=screen.getByRole("region", {name:"Reading streak"});expect(streak).toHaveTextContent("3days · current");expect(streak).toHaveTextContent("6days · longest");
 fireEvent.change(screen.getByRole("combobox", {name:"Heatmap metric"}),{target:{value:"characters"}});
 expect(screen.getAllByRole("button").find(button => button.getAttribute("aria-pressed")==="true")).toHaveAccessibleName(/1,234 characters/);
 fireEvent.change(screen.getByRole("combobox", {name:"Heatmap metric"}),{target:{value:"lookups"}});
 expect(screen.getAllByRole("button").find(button => button.getAttribute("aria-pressed")==="true")).toHaveAccessibleName(/7 lookups/);
});
