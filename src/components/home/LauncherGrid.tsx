import type { ReactNode } from "react";
import {
  AppsIcon,
  CleanerIcon,
  DevCleanerIcon,
  ExplorerIcon,
  HistoryIcon,
  ProtectedIcon,
} from "./ServiceIcons";

export type LauncherTarget =
  | "explorer"
  | "cleaner"
  | "dev_cleaner"
  | "apps"
  | "protected"
  | "history";

interface LauncherGridProps {
  activeScanRoot: string | null;
  indexedCount?: number;
  onNavigate: (page: LauncherTarget) => void;
}

interface LauncherItem {
  id: LauncherTarget;
  label: string;
  caption: string;
  badge?: string;
  icon: ReactNode;
}

export function LauncherGrid({ activeScanRoot, indexedCount = 0, onNavigate }: LauncherGridProps) {
  const items: LauncherItem[] = [
    {
      id: "explorer",
      label: "Explorador",
      caption:
        indexedCount > 1
          ? `${indexedCount} unidades listas`
          : activeScanRoot
            ? `Activa: ${activeScanRoot}`
            : "Treemap y ranking",
      badge:
        indexedCount > 1
          ? `${indexedCount} discos`
          : activeScanRoot
            ? activeScanRoot.replace("\\", "")
            : undefined,
      icon: <ExplorerIcon />,
    },
    {
      id: "cleaner",
      label: "Limpieza segura",
      caption: "Temporales y papelera",
      icon: <CleanerIcon />,
    },
    {
      id: "dev_cleaner",
      label: "Limpieza Dev",
      caption: "Cachés y artefactos",
      badge: "Dev",
      icon: <DevCleanerIcon />,
    },
    {
      id: "apps",
      label: "Aplicaciones",
      caption: "Win32 y Store / MSIX",
      icon: <AppsIcon />,
    },
    {
      id: "protected",
      label: "Protegidas",
      caption: "Lista negra inmutable",
      icon: <ProtectedIcon />,
    },
    {
      id: "history",
      label: "Historial",
      caption: "Snapshots y auditoría",
      icon: <HistoryIcon />,
    },
  ];

  return (
    <section aria-label="Accesos directos" className="w-full">
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4 sm:gap-5">
        {items.map((item, idx) => (
          <button
            key={item.id}
            type="button"
            onClick={() => onNavigate(item.id)}
            style={{ animationDelay: `${idx * 35}ms` }}
            className="group flex flex-col items-center gap-2.5 bg-transparent border-0 p-1 cursor-pointer focus-visible:outline-none animate-fade-up"
          >
            <div className="relative w-[84px] h-[84px] sm:w-[92px] sm:h-[92px] rounded-[24px] neu-tile grid place-items-center group-focus-visible:ring-2 group-focus-visible:ring-accent group-focus-visible:ring-offset-2 group-focus-visible:ring-offset-canvas">
              {item.badge && (
                <span className="absolute -top-2 -right-2 px-2 py-0.5 rounded-full text-[10.5px] font-semibold bg-accent-soft text-accent border border-accent-border backdrop-blur-sm shadow-xs">
                  {item.badge}
                </span>
              )}
              <div className="transition-transform duration-220 ease-out group-hover:scale-108">
                {item.icon}
              </div>
            </div>
            <div className="text-center">
              <div className="text-[13.5px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
                {item.label}
              </div>
              <div className="text-[11.5px] text-fg-muted mt-0.5 leading-snug">{item.caption}</div>
            </div>
          </button>
        ))}
      </div>
    </section>
  );
}
