import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import Catalog from "./Catalog";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
it("debounces catalog queries and ignores a stale response without fetching hidden covers", async () => {
  vi.useFakeTimers();
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  let oldResolve: (value: unknown) => void = () => {};
  const requests: Record<string, unknown>[] = [];
  vi.mocked(invoke).mockImplementation(async (_, input) => {
    const args = (input as { args: Record<string, unknown> }).args;
    requests.push(args);
    if (args.action === "browse") {
      if (args.query === "")
        return new Promise((resolve) => {
          oldResolve = resolve;
        });
      return {
        items: [
          {
            ns: "a".repeat(32),
            id: "b".repeat(32),
            title: "New result",
            creator: "",
            series: "",
            volume: "",
            bytes: 100,
            available: true,
            tags: [],
            collections: [],
          },
        ],
        next: null,
        ns: "a".repeat(32),
        namespaces: [],
      };
    }
    if (args.action === "cacheSettings") return { budget: 100000000, usage: 0 };
    return {};
  });
  render(<Catalog onOpen={vi.fn()} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(250);
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Search local catalog" }), {
    target: { value: "neko" },
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(250);
  });
  expect(screen.getByText("New result")).toBeTruthy();
  await act(async () => {
    oldResolve({ items: [], next: null, ns: "", namespaces: [] });
  });
  expect(screen.getByText("New result")).toBeTruthy();
  expect(requests.filter((r) => r.action === "browse")).toHaveLength(2);
  expect(requests.filter((r) => r.action === "cover")).toHaveLength(0);
});
