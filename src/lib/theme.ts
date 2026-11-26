import { useCallback, useEffect, useState } from "react";

export type ThemeMode = "light" | "dark";
export type ThemePreference = "system" | ThemeMode;

const STORAGE_KEY = "scancleaner-theme-pref";

function getSystemTheme(): ThemeMode {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function readStoredPref(): ThemePreference {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // Ignore storage access errors
  }
  return "system";
}

export function applyThemeToDocument(resolved: ThemeMode) {
  document.documentElement.setAttribute("data-theme", resolved);
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) {
    meta.setAttribute("content", resolved === "dark" ? "#0a0c13" : "#f4f3fb");
  }
}

export function useTheme() {
  const [preference, setPreference] = useState<ThemePreference>(() => readStoredPref());
  const [systemTheme, setSystemTheme] = useState<ThemeMode>(() => getSystemTheme());

  const resolved: ThemeMode = preference === "system" ? systemTheme : preference;

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystemTheme(e.matches ? "dark" : "light");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    applyThemeToDocument(resolved);
  }, [resolved]);

  const toggleTheme = useCallback(() => {
    setPreference((prev) => {
      const current = prev === "system" ? getSystemTheme() : prev;
      const next: ThemeMode = current === "dark" ? "light" : "dark";
      try {
        window.localStorage.setItem(STORAGE_KEY, next);
      } catch {
        // Ignore
      }
      return next;
    });
  }, []);

  return { preference, resolved, toggleTheme };
}
