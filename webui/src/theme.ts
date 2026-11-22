import { createSignal } from "solid-js";

export type Theme = "light" | "dark";

const STORAGE_KEY = "crabrouter_theme";

function detectInitialTheme(): Theme {
  if (typeof window !== "undefined") {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") {
      return saved;
    }
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      return "dark";
    }
  }
  return "dark";
}

function applyThemeToDom(next: Theme) {
  if (typeof document !== "undefined") {
    const root = document.documentElement;
    root.classList.toggle("dark", next === "dark");
    root.dataset.theme = next;
    root.style.colorScheme = next;
  }
}

const initial = detectInitialTheme();
applyThemeToDom(initial);

const [theme, setThemeSignal] = createSignal<Theme>(initial);

export function setTheme(next: Theme) {
  setThemeSignal(next);
  applyThemeToDom(next);
  if (typeof window !== "undefined") {
    window.localStorage.setItem(STORAGE_KEY, next);
  }
}

export function toggleTheme() {
  setTheme(theme() === "dark" ? "light" : "dark");
}

export { theme };
