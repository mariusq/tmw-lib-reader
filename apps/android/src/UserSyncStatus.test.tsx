import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import UserSyncStatus from "./UserSyncStatus";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(cleanup);
it("shows activity delivery separately and invokes the combined saved-data sync action", async () => {
  vi.mocked(invoke).mockResolvedValue({ busy: false, pending: 0, message: "Saved data synced", activity: { busy: false, pending: 3, rejected: 0, message: "Waiting for PC" } });
  await act(async () => { render(<UserSyncStatus />); });
  expect(screen.getByText("Reading activity: Waiting for PC · 3 pending")).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByText("Sync saved data now")); });
  expect(invoke).toHaveBeenCalledWith("mobile_storage", { args: { action: "userSync" } });
});
