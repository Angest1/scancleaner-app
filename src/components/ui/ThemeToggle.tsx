import { Moon, Sun } from "lucide-react";
import type { ThemeMode } from "../../lib/theme";

interface ThemeToggleProps {
  resolved: ThemeMode;
  onToggle: () => void;
}

export function ThemeToggle({ resolved, onToggle }: ThemeToggleProps) {
  const isDark = resolved === "dark";
  return (
    <button
      type="button"
      onClick={onToggle}
      title={isDark ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
      aria-label={isDark ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
      className="w-8 h-8 rounded-[9px] bg-elevated/85 border border-border-default hover:border-border-strong hover:bg-active grid place-items-center cursor-pointer shrink-0 transition-[background-color,border-color,transform] duration-140 active:scale-[0.97]"
    >
      {isDark ? (
        <Sun size={14} className="text-warning" aria-hidden="true" />
      ) : (
        <Moon size={14} className="text-accent" aria-hidden="true" />
      )}
    </button>
  );
}
