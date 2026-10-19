import { expect, it, vi } from "vitest";
import { PositionSync, type PositionSnapshot } from "./positionSync";
const position = (cfi: string, token = cfi, accepted = true): PositionSnapshot => ({ locationCfi: cfi, token, accepted });
const settle = async () => { for (let i=0;i<12;i++) await Promise.resolve(); };

it("blocks stale writes and further relocation until an explicit choice", async () => {
  const remote = position("phone", "remote", false);
  const read = vi.fn(async () => ({...remote, accepted: true}));
  const write = vi.fn(async () => remote);
  const conflict = vi.fn();
  const sync = new PositionSync(position("old"), read, write, conflict, vi.fn(), vi.fn());
  sync.save("desktop-next"); await settle();
  expect(conflict).toHaveBeenCalledWith(remote);
  sync.save("desktop-later"); await settle();
  expect(write).toHaveBeenCalledTimes(1);
  write.mockResolvedValue(position("desktop-later"));
  await sync.keep("desktop-later"); await settle();
  expect(write).toHaveBeenLastCalledWith("desktop-later", "remote");
});

it("coalesces pending navigation and serializes tokens through in-flight writes", async () => {
  let finish!: (value: PositionSnapshot) => void;
  const write = vi.fn().mockImplementationOnce(() => new Promise(resolve => { finish=resolve; }))
    .mockResolvedValue(position("last", "third"));
  const sync = new PositionSync(position("old", "first"), vi.fn(), write, vi.fn(), vi.fn(), vi.fn());
  sync.save("next"); sync.save("middle"); sync.save("last");
  expect(write).toHaveBeenCalledTimes(1);
  finish(position("next", "second")); await settle();
  expect(write).toHaveBeenLastCalledWith("last", "second");
  expect(write).toHaveBeenCalledTimes(2);
});

it("detects remote updates without writing and keeps a failed display blocked", async () => {
  const read = vi.fn(async () => position("phone"));
  const write = vi.fn(); const conflict=vi.fn();
  const sync = new PositionSync(position("old"), read, write, conflict, vi.fn(), vi.fn());
  sync.navigate();
  await sync.refresh();
  expect(conflict).toHaveBeenCalled(); expect(write).not.toHaveBeenCalled();
  await expect(sync.follow(async () => { throw Error("display failed"); })).rejects.toThrow("display failed");
  sync.save("old-page"); await settle(); expect(write).not.toHaveBeenCalled();
  await sync.follow(async next => { expect(next.locationCfi).toBe("phone"); });
  sync.save("phone"); await settle(); expect(write).not.toHaveBeenCalled();
});

it("retains a failed local save for retry and checks the original token", async () => {
  const write=vi.fn().mockRejectedValueOnce(Error("storage unavailable")).mockResolvedValue(position("next"));
  const failed=vi.fn();
  const sync=new PositionSync(position("old"),async () => position("old"),write,vi.fn(),failed,vi.fn());
  sync.save("next"); await settle(); expect(failed).toHaveBeenCalled();
  await sync.refresh(); await settle();
  expect(write).toHaveBeenLastCalledWith("next","old");
  expect(write).toHaveBeenCalledTimes(2);
});

it("silently adopts a same-CFI token change and uses it on the next write", async () => {
  const conflict = vi.fn(); const display = vi.fn();
  const write = vi.fn(async () => position("next"));
  const sync = new PositionSync(position("old"), async () => position("old", "promoted"), write, conflict, vi.fn(), display);
  sync.navigate();
  await sync.refresh();
  expect(sync.snapshot.token).toBe("promoted");
  expect(conflict).not.toHaveBeenCalled(); expect(display).not.toHaveBeenCalled();
  sync.save("next"); await settle();
  expect(write).toHaveBeenCalledWith("next", "promoted");
});

it("automatically follows remote changes without local navigation or a write", async () => {
  const next = position("phone");
  const display = vi.fn(async () => {}); const conflict = vi.fn(); const write = vi.fn();
  const sync = new PositionSync(position("old"), async () => next, write, conflict, vi.fn(), display);
  await sync.refresh();
  expect(display).toHaveBeenCalledWith(next); expect(sync.snapshot).toEqual(next);
  expect(conflict).not.toHaveBeenCalled(); expect(write).not.toHaveBeenCalled();
});

it("preserves deliberate backwards navigation even after its save completes", async () => {
  const write = vi.fn(async () => position("earlier"));
  const conflict = vi.fn(); const display = vi.fn();
  const sync = new PositionSync(position("old"), async () => position("phone"), write, conflict, vi.fn(), display);
  sync.save("earlier"); await settle(); await sync.refresh();
  expect(write).toHaveBeenCalledWith("earlier", "old");
  expect(conflict).toHaveBeenCalledWith(position("phone")); expect(display).not.toHaveBeenCalled();
});

it("checks navigation that starts during a poll before deciding to follow", async () => {
  let finish!: (value: PositionSnapshot) => void;
  const write = vi.fn(); const conflict = vi.fn(); const display = vi.fn();
  const sync = new PositionSync(position("old"), () => new Promise(resolve => { finish = resolve; }), write, conflict, vi.fn(), display);
  const refresh = sync.refresh();
  sync.save("local"); finish(position("phone")); await refresh;
  expect(conflict).toHaveBeenCalled(); expect(display).not.toHaveBeenCalled(); expect(write).not.toHaveBeenCalled();
});

it("blocks an atomic write rejection even when its CFI is unchanged", async () => {
  const conflict = vi.fn();
  const sync = new PositionSync(position("old"), vi.fn(), async () => position("old", "new", false), conflict, vi.fn(), vi.fn());
  sync.save("next"); await settle();
  expect(conflict).toHaveBeenCalledWith(position("old", "new", false));
});

it.each(["keep", "follow"] as const)("queues %s behind refresh and skips polls during the choice", async choice => {
  let finishPoll!: (value: PositionSnapshot) => void;
  let finishChoice!: (value: PositionSnapshot) => void;
  const read = vi.fn<() => Promise<PositionSnapshot>>()
    .mockImplementationOnce(() => new Promise(resolve => { finishPoll = resolve; }))
    .mockImplementationOnce(() => new Promise(resolve => { finishChoice = resolve; }));
  const write = vi.fn(async () => position("local"));
  const display = vi.fn(async () => {}); const failed = vi.fn();
  const sync = new PositionSync(position("old"), read, write, vi.fn(), failed, display);
  sync.navigate();
  const poll = sync.refresh();
  const decision = choice === "keep" ? sync.keep("local") : sync.follow(display);
  expect(read).toHaveBeenCalledTimes(1);
  finishPoll(position("phone")); await poll; await settle();
  expect(read).toHaveBeenCalledTimes(2);
  await sync.refresh();
  expect(read).toHaveBeenCalledTimes(2);
  finishChoice(position("latest"));
  await expect(decision).resolves.toBeUndefined();
  expect(failed).not.toHaveBeenCalled();
  if (choice === "keep") expect(write).toHaveBeenCalledWith("local", "latest");
  else { expect(display).toHaveBeenCalledWith(position("latest")); expect(write).not.toHaveBeenCalled(); }
});

it("keeps an intervening update protected during Keep", async () => {
  const conflict = vi.fn();
  const sync = new PositionSync(position("old"), async () => position("phone"), async () => position("newer", "newer", false), conflict, vi.fn(), vi.fn());
  await expect(sync.keep("local")).rejects.toThrow("Position could not be saved");
  expect(conflict).toHaveBeenCalledWith(position("newer", "newer", false));
});

it("keeps a failed automatic display blocked until a successful explicit follow", async () => {
  const display = vi.fn().mockRejectedValueOnce(Error("display failed")).mockResolvedValue(undefined);
  const conflict = vi.fn(); const failed = vi.fn(); const write = vi.fn();
  const sync = new PositionSync(position("old"), async () => position("phone"), write, conflict, failed, display);
  await sync.refresh();
  expect(conflict).toHaveBeenCalled(); expect(failed).toHaveBeenCalled();
  sync.save("old-page"); await settle(); expect(write).not.toHaveBeenCalled();
  await sync.follow(display);
  expect(sync.snapshot.locationCfi).toBe("phone");
});
