import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
import { LibraryShell } from "./LibraryShell";

class TestIntersectionObserver {
  observe() {}
  disconnect() {}
}
vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);

describe("LibraryShell", () => {
  it("renders the empty library state", () => {
    render(<LibraryShell />);
    expect(screen.getByRole("heading", { name: "すべての本" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "タイトル・著者・フォルダーを検索" })).toBeInTheDocument();
  });
});
