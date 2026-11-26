import { Moon, Sun } from "lucide-react";
import type { ThemeMode } from "../../lib/theme";
import { Button } from "./Button";

interface ThemeToggleProps {
  resolved: ThemeMode;
  onToggle: () => void;
}

export function ThemeToggle({ resolved, onToggle }: ThemeToggleProps) {
  const isDark = resolved === "dark";
  return (
    <Button
      variant="default"
      size="icon"
      onClick={onToggle}
      title={isDark ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
      aria-label={isDark ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
    >
      {isDark ? (
        <Sun size={14} className="text-warning" />
      ) : (
        <Moon size={14} className="text-accent" />
      )}
    </Button>
  );
}
