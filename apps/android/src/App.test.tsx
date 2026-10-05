import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => { throw new Error("Missing copy"); }) }));
vi.mock("./Catalog", () => ({ default: ({ onOpen }: { onOpen: (book: unknown) => void }) => <button onClick={() => onOpen({ name: "Replacement", bytes: new ArrayBuffer(0) })}>Open replacement</button> }));
vi.mock("./Reader", () => ({ default: () => <p>Replacement reader</p> }));
vi.mock("./ConnectionSettings", () => ({ default: () => null }));
vi.mock("./RecoverySettings", () => ({ default: () => null }));
vi.mock("./UserSyncStatus", () => ({ default: () => null }));
vi.mock("./LookupHistory", () => ({ default: () => null }));
afterEach(() => { localStorage.clear(); history.replaceState(null, ""); });
it("clears a failed download reopen when another book opens and forgets the invalid descriptor", async () => {
  localStorage.setItem("tmw-screen", "reader");
  localStorage.setItem("tmw-last-download", JSON.stringify({ file: "missing.epub", catalog: {} }));
  await act(async () => { render(<App />); });
  expect(screen.getByRole("alert").textContent).toContain("previous download");
  expect(localStorage.getItem("tmw-last-download")).toBeNull();
  fireEvent.click(screen.getByText("Open replacement"));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByText("Replacement reader")).toBeTruthy();
});
