import { invoke } from "@tauri-apps/api/core";
import type { LocalBook } from "./localBook";

export type BookIdentity = Omit<NonNullable<LocalBook["catalog"]>, "version"> & {
  version: string | null;
};
export type UserRecord = {
  entityId: string;
  contentVersion: string | null;
  deleted: boolean;
  fields: {
    locationCfi?: string;
    surface?: string;
    sentence?: string;
    note?: string;
    headword?: string | null;
    reading?: string | null;
  };
};
export type UserState = {
  progress: UserRecord | null;
  passages: UserRecord[];
  pending: number;
  next: number | null;
};
export type SyncStatus = {
  busy: boolean;
  pending: number;
  message: string;
  rejected?: {
    id: string;
    reason: string;
    operation?: { kind: string; bookId: string; fields: UserRecord["fields"] };
  }[];
  lastSuccess?: number;
  nextRetry?: number;
};
export const mobileUser = <T>(args: Record<string, unknown>) =>
  invoke<T>("mobile_storage", { args });
export const readUserState = (book: BookIdentity, offset = 0) =>
  mobileUser<UserState>({ action: "userState", ...book, offset });
export const saveUserData = (
  book: BookIdentity,
  kind: "progress" | "passage",
  fields: UserRecord["fields"],
  entityId?: string,
  deleted = false,
) =>
  mobileUser<{ entityId: string }>({
    action: "userSave",
    ...book,
    kind,
    fields,
    entityId,
    deleted,
  });
