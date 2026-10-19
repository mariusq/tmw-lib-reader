import { afterEach, expect, it } from "vitest";
import { APP_THEME_KEY, appTheme, applyAppTheme, saveAppTheme, storedAppTheme } from "./appTheme";

afterEach(() => { localStorage.clear(); delete document.documentElement.dataset.appTheme; });

it("defaults unknown or missing schemes to dark", () => {
  expect(appTheme("pastel")).toBe("pastel");
  expect(appTheme("toString")).toBe("dark");
  expect(appTheme(null)).toBe("dark");
  expect(storedAppTheme()).toBe("dark");
});
it("marks the root element and remembers the choice", () => {
  saveAppTheme("pastelNight");
  expect(document.documentElement.dataset.appTheme).toBe("pastelNight");
  expect(localStorage.getItem(APP_THEME_KEY)).toBe("pastelNight");
  expect(storedAppTheme()).toBe("pastelNight");
  applyAppTheme("dark");
  expect(document.documentElement.dataset.appTheme).toBeUndefined();
});
