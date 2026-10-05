import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import RecoverySettings, { recoveryReaderSettings } from "./RecoverySettings";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => { localStorage.clear(); invoke.mockReset(); });
test("exports offline reader state without unrelated WebView data", async () => {
  localStorage.setItem("tmw-local-cfi-proof", "epubcfi(/6/2)");
  localStorage.setItem("unrelated-secret", "excluded");
  expect(recoveryReaderSettings(localStorage)).toEqual({ "tmw-local-cfi-proof": "epubcfi(/6/2)" });
  invoke.mockResolvedValue({ message: "Recovery export saved" });
  render(<RecoverySettings />);
  fireEvent.click(screen.getByText("Export phone data"));
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("saved"));
  expect(invoke).toHaveBeenCalledWith("mobile_storage", { args: {
    action: "exportRecovery", webState: { "tmw-local-cfi-proof": "epubcfi(/6/2)" },
  } });
});
