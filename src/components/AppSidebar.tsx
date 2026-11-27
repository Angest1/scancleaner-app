import {
  AppWindow,
  Code2,
  FolderTree,
  History,
  Home,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { ElevationStatus, SyncStatusDto } from "../lib/ipc";
import type { NavPage } from "./AppHeader";
import { ElevationBanner } from "./ElevationBanner";
import { SyncStatusPill } from "./SyncStatusPill";
import { BrandMark } from "./ui/BrandMark";
import { ThemeToggle } from "./ui/ThemeToggle";

const NAV_GROUPS: {
  title: string;
  items: { id: NavPage; label: string; icon: typeof Home }[];
}[] = [
  {
    title: "Análisis",
    items: [
      { id: "home", label: "Inicio", icon: Home },
      { id: "explorer", label: "Explorador", icon: FolderTree },
    ],
  },
  {
    title: "Limpieza",
    items: [
      { id: "cleaner", label: "Limpieza segura", icon: Sparkles },
      { id: "dev_cleaner", label: "Limpieza Dev", icon: Code2 },
      { id: "apps", label: "Aplicaciones", icon: AppWindow },
    ],
  },
  {
    title: "Sistema",
    items: [
      { id: "protected", label: "Rutas protegidas", icon: ShieldCheck },
      { id: "history", label: "Historial", icon: History },
      { id: "settings", label: "Ajustes", icon: Settings },
    ],
  },
];

interface AppSidebarProps {
  page: NavPage;
  collapsed: boolean;
  scanningDrive: string | null;
  activeDriveRoot: string | null;
  syncStatus: SyncStatusDto;
  elevation: ElevationStatus | null;
  theme: "light" | "dark";
  onSelectPage: (next: NavPage) => void;
  onToggleCollapse: () => void;
  onManualSync: () => void;
  onRelaunchAdmin: () => void;
  onToggleTheme: () => void;
}

/**
 * Wide-screen (`xl:`) desktop navigation sidebar with unified 9px radius geometry,
 * subtle active left indicator, collapsible 68px icon rail, and flush utility footer.
 */
export function AppSidebar({
  page,
  collapsed,
  scanningDrive,
  activeDriveRoot,
  syncStatus,
  elevation,
  theme,
  onSelectPage,
  onToggleCollapse,
  onManualSync,
  onRelaunchAdmin,
  onToggleTheme,
}: AppSidebarProps) {
  return (
    <aside
      aria-label="Barra lateral de navegación"
      className={`hidden xl:flex relative z-30 shrink-0 h-full flex-col justify-between bg-shell backdrop-blur-md border-r border-border-subtle transition-[width] duration-200 ease-out ${
        collapsed ? "w-[68px] px-2.5" : "w-[232px] px-3"
      } py-3.5`}
    >
      {/* Top brand + collapse control */}
      <div className="flex flex-col gap-5 min-h-0 flex-1">
        <div
          className={`flex items-center ${
            collapsed ? "flex-col gap-2.5" : "justify-between gap-2 px-0.5"
          }`}
        >
          <button
            type="button"
            onClick={() => onSelectPage("home")}
            className="flex items-center gap-2.5 bg-transparent border-0 p-0.5 rounded-[9px] cursor-pointer hover:opacity-90 transition-opacity min-w-0"
            title="Ir a Inicio"
          >
            <BrandMark size={28} />
            {!collapsed && (
              <div className="text-left leading-tight min-w-0">
                <div
                  className="text-[14px] font-bold tracking-[-0.02em] text-fg-primary truncate"
                  translate="no"
                >
                  ScanCleaner
                </div>
                <div className="text-[10.5px] text-fg-muted truncate">
                  {scanningDrive
                    ? `Escaneando ${scanningDrive}`
                    : activeDriveRoot
                      ? `Unidad: ${activeDriveRoot}`
                      : "Windows Disk Suite"}
                </div>
              </div>
            )}
          </button>

          <button
            type="button"
            onClick={onToggleCollapse}
            title={collapsed ? "Expandir barra lateral" : "Contraer barra lateral"}
            aria-label={collapsed ? "Expandir barra lateral" : "Contraer barra lateral"}
            className="w-8 h-8 rounded-[9px] bg-transparent hover:bg-elevated border border-transparent hover:border-border-default grid place-items-center text-fg-muted hover:text-fg-primary cursor-pointer shrink-0 transition-[background-color,color,border-color,transform] duration-140 active:scale-[0.97]"
          >
            {collapsed ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
          </button>
        </div>

        {/* Grouped navigation links */}
        <nav
          aria-label="Navegación principal"
          className="flex flex-col gap-4 overflow-y-auto pr-0.5"
        >
          {NAV_GROUPS.map((group) => (
            <div
              key={group.title}
              className={`flex flex-col gap-1 ${collapsed ? "items-center" : ""}`}
            >
              {!collapsed && (
                <div className="px-2.5 pb-0.5 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-fg-muted">
                  {group.title}
                </div>
              )}
              {group.items.map((item) => {
                const Icon = item.icon;
                const active = page === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    aria-current={active ? "page" : undefined}
                    title={collapsed ? item.label : undefined}
                    onClick={() => onSelectPage(item.id)}
                    className={`relative flex items-center ${
                      collapsed ? "w-9 h-9 justify-center px-0" : "w-full h-9 gap-2.5 px-2.5"
                    } rounded-[9px] text-[12.5px] font-medium border cursor-pointer transition-[background-color,color,border-color,transform] duration-140 active:scale-[0.97] ${
                      active
                        ? "bg-tile text-accent border-accent-border shadow-[0_1px_6px_-2px_rgba(59,110,245,0.22)]"
                        : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary hover:bg-elevated/80"
                    }`}
                  >
                    {active && !collapsed && (
                      <span
                        aria-hidden="true"
                        className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r-full bg-accent"
                      />
                    )}
                    <Icon size={15} className="shrink-0" aria-hidden="true" />
                    {!collapsed && <span className="truncate">{item.label}</span>}
                  </button>
                );
              })}
            </div>
          ))}
        </nav>
      </div>

      {/* Uniform flush bottom utility footer (every control is 32px / 9px radius) */}
      <div
        className={`pt-3 mt-3 border-t border-border-subtle flex flex-col gap-2 ${
          collapsed ? "items-center" : ""
        }`}
      >
        {collapsed ? (
          <>
            <SyncStatusPill status={syncStatus} onManualSync={onManualSync} iconOnly />
            <ElevationBanner
              elevation={elevation}
              activeDrive={activeDriveRoot}
              onRelaunch={onRelaunchAdmin}
              layout="icon"
            />
            <ThemeToggle resolved={theme} onToggle={onToggleTheme} />
          </>
        ) : (
          <>
            <div className="flex items-center gap-2 w-full">
              <SyncStatusPill
                status={syncStatus}
                onManualSync={onManualSync}
                className="flex-1 justify-start"
              />
              <ThemeToggle resolved={theme} onToggle={onToggleTheme} />
            </div>
            <ElevationBanner
              elevation={elevation}
              activeDrive={activeDriveRoot}
              onRelaunch={onRelaunchAdmin}
              layout="sidebar"
            />
          </>
        )}
      </div>
    </aside>
  );
}
