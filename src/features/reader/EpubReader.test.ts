import { describe, expect, it, vi } from "vitest";
import { readerRenditionOptions } from "./readerConfig";
import { turnReaderPage, type PageLocation } from "./readerNavigation";

function pageLocation(page: number, total = 3, index = 0): PageLocation {
  const position = { index, cfi: `section-${index}-page-${page}`, displayed: { page, total } };
  return { start: { ...position }, end: { ...position } };
}

describe("reader page navigation", () => {
  it("advances one page without jumping chapters while relocation is delayed", async () => {
    let location = pageLocation(1);
    let relocate = () => {};
    const view = {
      next: vi.fn(async () => { relocate = () => { location = pageLocation(2); }; }),
      prev: vi.fn(async () => {}),
      display: vi.fn(),
    };
    await turnReaderPage(view, "next", {
      location: () => location,
      nextSection: view.display,
      settle: async () => { expect(location).toEqual(pageLocation(1)); relocate(); },
    });
    expect(view.next).toHaveBeenCalledTimes(1);
    expect(view.prev).not.toHaveBeenCalled();
    expect(view.display).not.toHaveBeenCalled();
    expect(location).toEqual(pageLocation(2));
  });

  it("recovers a stuck final page after the turn settles", async () => {
    const view = { next: vi.fn(async () => {}), prev: vi.fn(async () => {}) };
    const nextSection = vi.fn(async () => {});
    await turnReaderPage(view, "next", {
      location: () => pageLocation(3, 3, 4), nextSection, settle: async () => {},
    });
    expect(view.next).toHaveBeenCalledTimes(1);
    expect(nextSection).toHaveBeenCalledExactlyOnceWith(4);
  });

  it("never jumps from a stuck interior page or an unknown location", async () => {
    const view = { next: vi.fn(async () => {}), prev: vi.fn(async () => {}) };
    const nextSection = vi.fn(async () => {});
    for (const location of [pageLocation(2), pageLocation(1, 0), undefined]) {
      await turnReaderPage(view, "next", { location: () => location, nextSection, settle: async () => {} });
    }
    expect(nextSection).not.toHaveBeenCalled();
  });

  it("does not skip another section when the normal boundary turn succeeds", async () => {
    let location = pageLocation(3);
    const view = { next: vi.fn(async () => { location = pageLocation(1, 1, 1); }), prev: vi.fn(async () => {}) };
    const nextSection = vi.fn(async () => {});
    await turnReaderPage(view, "next", { location: () => location, nextSection, settle: async () => {} });
    expect(nextSection).not.toHaveBeenCalled();
  });

  it("ignores overlapping turns and releases the guard after failure", async () => {
    let finish = () => {};
    const view = { next: vi.fn(() => new Promise<void>((resolve) => { finish = resolve; })), prev: vi.fn(async () => {}) };
    const first = turnReaderPage(view, "next");
    await turnReaderPage(view, "next");
    await turnReaderPage(view, "previous");
    expect(view.next).toHaveBeenCalledTimes(1);
    expect(view.prev).not.toHaveBeenCalled();
    finish();
    await first;
    view.next.mockRejectedValueOnce(new Error("turn failed"));
    await expect(turnReaderPage(view, "next")).rejects.toThrow("turn failed");
    await turnReaderPage(view, "previous");
    expect(view.prev).toHaveBeenCalledTimes(1);
  });

  it("retreats through the page manager once", async () => {
    const view = { next: vi.fn(async () => {}), prev: vi.fn(async () => {}) };
    await turnReaderPage(view, "previous");
    expect(view.prev).toHaveBeenCalledTimes(1);
    expect(view.next).not.toHaveBeenCalled();
  });
});

describe("readerRenditionOptions", () => {
  it("does not override publication layout, spread, flow, or direction metadata", () => {
    const options = readerRenditionOptions();

    expect(options).toEqual({ width: "100%", height: "100%", manager: "default" });
    expect(options).not.toHaveProperty("layout");
    expect(options).not.toHaveProperty("spread");
    expect(options).not.toHaveProperty("flow");
    expect(options).not.toHaveProperty("defaultDirection");
  });
});
