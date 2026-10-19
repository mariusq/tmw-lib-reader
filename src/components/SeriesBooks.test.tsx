import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SeriesBooks } from "./SeriesBooks";
import { groupSeriesCopies, type SeriesBook } from "./seriesCopyGroups";

const book = (id: number, values: Partial<SeriesBook> = {}): SeriesBook => ({ id, effectiveTitle: "Story 1", effectiveCreator: "Author", effectiveVolume: "1", effectiveCoverPath: null, isAvailable: true, isFinished: false, ...values });

describe("Series copies", () => {
  it("keeps editions, bonus stories, authors and volume overrides distinct and prefers available copies", () => {
    const groups = groupSeriesCopies([book(1, { isAvailable: false }), book(2), book(3, { effectiveTitle: "Story 1 revised edition" }), book(4, { effectiveTitle: "Story 1 bonus" }), book(5, { effectiveCreator: "Other" }), book(6, { effectiveVolume: "2" })]);
    expect(groups).toHaveLength(5);
    expect(groups[0].map(copy => copy.id)).toEqual([2, 1]);
  });
  it("switches between grid and list while retaining copy groups", () => {
    const props = { books: [book(1), book(2)], groupCopies: true, showCovers: false, language: "en" as const, hasMore: false, onOpen: vi.fn() };
    const { container, rerender } = render(<SeriesBooks {...props} layout="grid" />);
    expect(container.querySelector('[data-layout="grid"]')).toHaveClass("library-grid");
    expect(screen.getByText("Show 2 copies")).toBeInTheDocument();
    rerender(<SeriesBooks {...props} layout="list" />);
    expect(container.querySelector('[data-layout="list"]')).toHaveClass("space-y-3");
    expect(screen.getByText("Show 2 copies")).toBeInTheDocument();
  });
  it("opens each retained copy and supports showing them individually", () => {
    const onOpen = vi.fn();
    const books = [book(1, { isAvailable: false }), book(2, { isFinished: true })];
    const { rerender } = render(<SeriesBooks books={books} groupCopies showCovers={false} language="en" hasMore={false} onOpen={onOpen} />);
    expect(screen.getAllByRole("heading", { name: "Story 1" })).toHaveLength(1);
    expect(screen.getByText("1 titles · 2 copies")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Show 2 copies"));
    fireEvent.click(screen.getByRole("button", { name: /Copy 2 · #1/ }));
    expect(onOpen).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByRole("button", { name: /Copy 1 · #2/ }));
    expect(onOpen).toHaveBeenCalledWith(2);
    rerender(<SeriesBooks books={books} groupCopies={false} showCovers={false} language="en" hasMore={false} onOpen={onOpen} />);
    expect(screen.getAllByRole("heading", { name: "Story 1" })).toHaveLength(2);
  });
});
