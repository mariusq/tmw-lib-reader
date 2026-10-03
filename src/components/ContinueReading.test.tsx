import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ContinueReading } from "./ContinueReading";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock("../features/reader/EpubReader", () => ({ EpubReader: ({ bookId, onClose }: { bookId: number; onClose: () => void }) => <div>Reader {bookId}<button onClick={onClose}>Close reader</button></div> }));
const recent = { id: 2, title: "物語", creator: "著者", coverPath: null, lastReadAt: 100, hasLocation: true, isAvailable: false };
const available = { ...recent, id: 1, title: "Available", isAvailable: true };

beforeEach(() => { vi.mocked(invoke).mockReset(); });
describe("ContinueReading", () => {
  it("resumes the available candidate, retains unavailable books and refreshes only on close", async () => {
    vi.mocked(invoke).mockImplementation(async (_, args) => (args as { availableOnly: boolean }).availableOnly ? [available] : [recent]);
    const roots = vi.fn();
    render(<ContinueReading language="en" onLibrary={vi.fn()} onRoots={roots} />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue reading: Available" }));
    expect(screen.getByText("Reader 1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Resume: 物語" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Library roots" }));
    expect(roots).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledTimes(2);
    fireEvent.click(screen.getByRole("button", { name: "Close reader" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(4));
  });
  it("offers library and root routes for a new reader", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const library = vi.fn(), roots = vi.fn();
    render(<ContinueReading language="en" onLibrary={library} onRoots={roots} />);
    expect(await screen.findByText("Open a book from your library and it will appear here next time.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Browse full library" }));
    fireEvent.click(screen.getByRole("button", { name: "Add a library folder" }));
    expect(library).toHaveBeenCalledOnce(); expect(roots).toHaveBeenCalledOnce();
  });
});
