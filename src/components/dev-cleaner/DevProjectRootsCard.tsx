import { FolderPlus, Trash2 } from "lucide-react";
import { useState } from "react";
import type { DevCleanerConfig } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";

interface DevProjectRootsCardProps {
  config: DevCleanerConfig;
  onPickFolder: () => void;
  onAddManualRoot: (root: string) => void;
  onRemoveRoot: (root: string) => void;
  onChangeAgeDays: (days: number) => void;
}

export function DevProjectRootsCard({
  config,
  onPickFolder,
  onAddManualRoot,
  onRemoveRoot,
  onChangeAgeDays,
}: DevProjectRootsCardProps) {
  const [manualRoot, setManualRoot] = useState("");

  const handleSubmit = () => {
    const trimmed = manualRoot.trim();
    if (!trimmed) return;
    onAddManualRoot(trimmed);
    setManualRoot("");
  };

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <div className="font-semibold text-[13.5px] text-fg-primary">
            Raíces de proyectos para artefactos (node_modules, vendor, .venv, build, target)
          </div>
          <div className="text-[12px] text-fg-secondary mt-0.5">
            Solo se escanean estas carpetas (nunca el disco completo ni dentro de artefactos o
            symlinks).
          </div>
        </div>
        <label className="text-[12px] text-fg-secondary flex items-center gap-1.5 shrink-0">
          Antigüedad mín. (días)
          <input
            type="number"
            className="w-[68px] h-8 px-2 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary focus:border-accent focus:outline-none"
            min={0}
            value={config.minProjectAgeDays}
            onChange={(e) => onChangeAgeDays(Math.max(0, Number(e.target.value) || 0))}
          />
        </label>
      </div>

      <div className="flex items-center gap-2 flex-wrap">
        <Button onClick={onPickFolder}>
          <FolderPlus size={13} />
          Elegir carpeta...
        </Button>
        <input
          type="text"
          name="project-root"
          autoComplete="off"
          spellCheck={false}
          aria-label="Ruta de carpeta de proyectos"
          className="flex-1 min-w-[220px] h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus-visible:ring-2 focus-visible:ring-accent/30"
          placeholder="O pega una ruta (ej. D:\proyectos)"
          value={manualRoot}
          onChange={(e) => setManualRoot(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSubmit();
          }}
        />
        <Button onClick={handleSubmit}>Agregar</Button>
      </div>

      {config.projectRoots.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {config.projectRoots.map((root) => (
            <span
              key={root}
              className="inline-flex items-center gap-1.5 mono text-[11.5px] px-2.5 py-1 rounded-[6px] bg-canvas border border-border-default text-fg-secondary"
            >
              <span>{root}</span>
              <button
                type="button"
                onClick={() => onRemoveRoot(root)}
                title="Quitar raíz"
                aria-label={`Quitar raíz ${root}`}
                className="inline-flex text-fg-muted hover:text-danger bg-transparent border-0 p-0 cursor-pointer transition-colors"
              >
                <Trash2 size={12} />
              </button>
            </span>
          ))}
        </div>
      )}
    </Card>
  );
}
