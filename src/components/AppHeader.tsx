import {
  AppWindow,
  Code2,
  FolderTree,
  History,
  Home,
  Menu,
  Settings,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import type { ElevationStatus, SyncStatusDto } from "../lib/ipc";
import { ElevationBanner } from "./ElevationBanner";
import { SyncStatusPill } from "./SyncStatusPill";
import { BrandMark } from "./ui/BrandMark";
import { Button } from "./ui/Button";
import { ThemeToggle } from "./ui/ThemeToggle";

export type NavPage =
  | "home"
  | "explorer"
  | "cleaner"
  | "dev_cleaner"
  | "apps"
  | "protected"
  | "history"
  | "settings";

const NAV_ITEMS: { id: NavPage; label: string; icon: typeof Home }[] = [
  { id: "home", label: "Inicio", icon: Home },
  { id: "explorer", label: "Explorador", icon: FolderTree },
  { id: "cleaner", label: "Limpieza", icon: Sparkles },
  { id: "dev_cleaner", label: "Dev", icon: Code2 },
  { id: "apps", label: "Apps", icon: AppWindow },
  { id: "protected", label: "Protegidas", icon: ShieldCheck },
  { id: "history", label: "Historial", icon: History },
  { id: "settings", label: "Ajustes", icon: Settings },
];

interface AppHeaderProps {
  page: NavPage;
  mobileMenuOpen: boolean;
  scanningDrive: string | null;
  activeDriveRoot: string | null;
  syncStatus: SyncStatusDto;
  elevation: ElevationStatus | null;
  theme: "light" | "dark";
  onSelectPage: (next: NavPage) => void;
  onToggleMobileMenu: () => void;
  onManualSync: () => void;
  onRelaunchAdmin: () => void;
  onToggleTheme: () => void;
}

export function AppHeader({
  page,
  mobileMenuOpen,
  scanningDrive,
  activeDriveRoot,
  syncStatus,
  elevation,
  theme,
  onSelectPage,
  onToggleMobileMenu,
  onManualSync,
  onRelaunchAdmin,
  onToggleTheme,
}: AppHeaderProps) {
  return (
    <header className="relative z-30 shrink-0 h-14 px-4 sm:px-6 bg-shell backdrop-blur-md border-b border-border-subtle">
      <div className="h-full max-w-[1440px] mx-auto flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <button
            type="button"
            onClick={() => onSelectPage("home")}
            className="flex items-center gap-2.5 bg-transparent border-0 p-1 -ml-1 rounded-[10px] cursor-pointer hover:opacity-90 transition-opacity"
            title="Ir a Inicio"
          >
            <BrandMark size={30} />
            <div className="text-left leading-tight">
              <div
                className="text-[14.5px] font-bold tracking-[-0.02em] text-fg-primary"
                translate="no"
              >
                ScanCleaner
              </div>
              <div className="text-[10.5px] text-fg-muted hidden sm:block">
                {scanningDrive
                  ? `Escaneando ${scanningDrive}`
                  : activeDriveRoot
                    ? `Unidad: ${activeDriveRoot}`
                    : "Windows Disk Suite"}
              </div>
            </div>
          </button>
        </div>

        <nav
          aria-label="Navegación principal"
          className="hidden lg:flex items-center gap-0.5 p-1 rounded-full bg-elevated/90 border border-border-default shadow-[0_2px_10px_-4px_rgba(32,28,76,0.12)]"
        >
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active = page === item.id;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => onSelectPage(item.id)}
                className={`inline-flex items-center gap-1.5 h-7.5 px-2.5 rounded-full text-[12px] font-medium cursor-pointer border transition-[background-color,color,border-color,box-shadow] duration-140 ${
                  active
                    ? "bg-tile text-accent border-accent-border shadow-[0_2px_8px_-2px_rgba(59,110,245,0.25)]"
                    : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary hover:bg-panel/60"
                }`}
              >
                <Icon size={13} className="shrink-0" aria-hidden="true" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="flex items-center gap-2 shrink-0">
          <SyncStatusPill status={syncStatus} onManualSync={onManualSync} />
          <ElevationBanner
            elevation={elevation}
            activeDrive={activeDriveRoot}
            onRelaunch={onRelaunchAdmin}
          />
          <ThemeToggle resolved={theme} onToggle={onToggleTheme} />
          <Button
            variant="default"
            size="icon"
            className="lg:hidden"
            onClick={onToggleMobileMenu}
            aria-label={mobileMenuOpen ? "Cerrar menú" : "Abrir menú"}
          >
            {mobileMenuOpen ? <X size={15} /> : <Menu size={15} />}
          </Button>
        </div>
      </div>

      {mobileMenuOpen && (
        <div className="lg:hidden absolute left-4 right-4 top-[60px] z-50 rounded-[14px] bg-tile border border-border-default p-2 grid grid-cols-2 sm:grid-cols-4 gap-1.5 animate-popover-in shadow-[var(--shadow-floating)]">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active = page === item.id;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => onSelectPage(item.id)}
                className={`flex items-center gap-2 h-9 px-3 rounded-[9px] text-[12.5px] font-medium border cursor-pointer ${
                  active
                    ? "bg-accent-soft text-accent border-accent-border"
                    : "bg-transparent text-fg-secondary border-transparent hover:bg-elevated hover:text-fg-primary"
                }`}
              >
                <Icon size={14} aria-hidden="true" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>
      )}
    </header>
  );
}
