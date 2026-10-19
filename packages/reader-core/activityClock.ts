import { readingIntervals } from "./readingActivity";

/** Foreground comes from the host activity, never EPUB document focus. */
export class ActivityClock {
  private wall: number;
  private mono: number;
  private lastInput: number;
  private foreground: boolean;
  dwell = 0;
  constructor(wall: number, mono: number, foreground: boolean) {
    this.wall = wall; this.mono = mono; this.lastInput = mono; this.foreground = foreground;
  }
  sample(wall: number, mono: number) {
    const delta = mono - this.mono;
    const elapsed = Math.min(delta, Math.max(0, this.lastInput + 1800000 - this.mono));
    const intervals = this.foreground && delta >= 0 && delta <= 15000 && Math.abs(wall - this.wall - delta) <= 1000
      ? readingIntervals(this.wall, wall - (delta - elapsed), elapsed) : [];
    if (delta < 0 || delta > 15000 || Math.abs(wall - this.wall - delta) > 1000) this.dwell = 0;
    this.dwell += intervals.reduce((sum, interval) => sum + interval.milliseconds, 0);
    this.wall = wall; this.mono = mono;
    return intervals;
  }
  input(mono: number) { this.lastInput = mono; }
  focus(value: boolean, mono: number) {
    if (value !== this.foreground) { this.dwell = 0; if (value) this.lastInput = mono; }
    this.foreground = value;
  }
  page() { this.dwell = 0; }
  canCredit(mono: number) { return this.foreground && this.dwell >= 3000 && mono - this.lastInput <= 1800000; }
}
