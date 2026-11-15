import { beforeEach, describe, expect, it } from "vitest";
import {
  applyTheme,
  getSelectedTheme,
  isThemeId,
  setSelectedTheme,
  THEMES,
} from "@/features/settings/theme";

describe("theme", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.colorScheme = "";
  });

  it("falls back to the default theme", () => {
    expect(getSelectedTheme()).toBe("forest");
  });

  it("persists and restores the chosen theme", () => {
    setSelectedTheme("archive");
    expect(localStorage.getItem("pms.theme")).toBe("archive");
    expect(getSelectedTheme()).toBe("archive");
  });

  it("ignores unknown stored values", () => {
    localStorage.setItem("pms.theme", "neon-pink");
    expect(getSelectedTheme()).toBe("forest");
    expect(isThemeId("neon-pink")).toBe(false);
  });

  it("applies data-theme and color-scheme", () => {
    applyTheme("terminal");
    expect(document.documentElement.dataset.theme).toBe("terminal");
    expect(document.documentElement.style.colorScheme).toBe("dark");

    applyTheme("linen");
    expect(document.documentElement.style.colorScheme).toBe("light");

    applyTheme("forest");
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("offers distinct options", () => {
    const ids = THEMES.map((theme) => theme.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain("forest");
    for (const theme of THEMES) {
      expect(theme.swatch).toHaveLength(3);
    }
  });
});
