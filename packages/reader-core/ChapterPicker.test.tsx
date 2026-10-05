import { render, screen, fireEvent } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ChapterPicker from "./ChapterPicker";

it("keeps nested chapter anchors intact and excludes external destinations", () => {
  const jump = vi.fn();
  render(<ChapterPicker ready pending={false} onJump={jump} chapters={[
    { label: "第一章", href: "Text/chapter.xhtml", subitems: [
      { label: "Section two", href: "Text/chapter.xhtml#two" },
    ] },
    { label: "External", href: "https://example.com" },
  ]} />);
  fireEvent.click(screen.getByText("Go to chapter"));
  fireEvent.click(screen.getByRole("button", { name: "Section two" }));
  expect(jump).toHaveBeenCalledWith("Text/chapter.xhtml#two");
  expect(screen.queryByRole("button", { name: "External" })).toBeNull();
});

it("explains missing contents", () => {
  render(<ChapterPicker ready pending={false} onJump={() => {}} chapters={[]} />);
  fireEvent.click(screen.getByText("Go to chapter"));
  expect(screen.getByText("This book has no table of contents.")).toBeTruthy();
});
