export type PositionSnapshot = { locationCfi: string | null; token: string; accepted: boolean };

/** Serialize writes and choices, preserving deliberate local navigation on conflict. */
export class PositionSync {
  private pending: string | null = null;
  private operation: Promise<void> | null = null;
  private blocked = false;
  private navigated = false;
  constructor(private current: PositionSnapshot,
    private read: () => Promise<PositionSnapshot>,
    private write: (cfi: string, token: string) => Promise<PositionSnapshot>,
    private conflict: (position: PositionSnapshot) => void,
    private failed: (reason: unknown) => void,
    private display: (position: PositionSnapshot) => Promise<void>) {}
  get snapshot() { return this.current; }
  block(next: PositionSnapshot) {
    this.current = next; this.blocked = true; this.pending = null; this.conflict(next);
  }

  navigate() { this.navigated = true; }

  private run(action: () => Promise<void>) {
    const task = this.operation ? this.operation.then(action) : action();
    const result = task.finally(() => { if (this.operation === settled) this.operation = null; });
    const settled = result.then(() => {}, () => {});
    this.operation = settled;
    return result;
  }

  save(cfi: string) {
    this.navigate();
    if (this.blocked || (!this.operation && this.pending === null && cfi === this.current.locationCfi)) return;
    this.pending = cfi;
    void this.drain();
  }
  private async drain() {
    if (this.operation || this.blocked || this.pending === null) return;
    await this.run(() => this.flush());
  }
  private async flush() {
    try {
      while (this.pending !== null && !this.blocked) {
        const cfi = this.pending;
        this.pending = null;
        if (cfi === this.current.locationCfi) continue;
        let result: PositionSnapshot;
        try { result = await this.write(cfi, this.current.token); }
        catch (reason) { this.pending ??= cfi; throw reason; }
        this.current = result;
        if (!result.accepted) {
          this.block(result);
        }
      }
    } catch (reason) { this.failed(reason); }
  }
  async refresh() {
    if (this.operation) return;
    // Hold the queue during the read so a stale poll cannot invalidate our own save.
    await this.run(async () => {
      try {
        const next = await this.read();
        if (next.token !== this.current.token) {
          if (next.locationCfi === this.current.locationCfi) {
            this.current = next;
          } else if (!this.blocked && !this.navigated && this.pending === null && next.locationCfi) {
            // No deliberate local position to lose. Displaying never creates a write.
            try { await this.display(next); this.current = next; }
            catch (reason) { this.block(next); throw reason; }
          } else this.block(next);
        }
      } catch (reason) { this.failed(reason); }
    });
    void this.drain();
  }
  async follow(display: (position: PositionSnapshot) => Promise<void>) {
    await this.run(async () => {
      this.blocked = true;
      this.pending = null;
      try {
        const next = await this.read();
        if (!next.locationCfi) throw new Error("Synced position is unavailable for this book version.");
        await display(next);
        this.current = next; this.blocked = false; this.navigated = false;
      } catch (reason) { this.blocked = true; throw reason; }
    });
    void this.drain();
  }
  async keep(cfi: string) {
    await this.run(async () => {
      this.current = await this.read(); this.blocked = false;
      this.navigated = true; this.pending = cfi;
      await this.flush();
      if (this.blocked || this.pending !== null) throw new Error("Position could not be saved. Check sync and try again.");
    });
  }
}
