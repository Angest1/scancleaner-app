import { FilePlus2, FolderPlus, Plus, SearchCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { type ProtectionResult, type ProtectionType, api } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { AlertBanner } from "../ui/Feedback";

interface ProtectedRuleFormsProps {
  onAdded: () => void;
  onError: (msg: string) => void;
}

export function ProtectedRuleForms({ onAdded, onError }: ProtectedRuleFormsProps) {
  const [patron, setPatron] = useState("");
  const [tipo, setTipo] = useState<ProtectionType>("carpeta-recursiva");
  const [motivo, setMotivo] = useState("");
  const [allDrives, setAllDrives] = useState(false);
  const [previewCount, setPreviewCount] = useState<number | null>(null);
  const [testPath, setTestPath] = useState("");
  const [testResult, setTestResult] = useState<ProtectionResult | null>(null);

  useEffect(() => {
    const trimmed = patron.trim();
    if (!trimmed) {
      setPreviewCount(null);
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .protectionPreviewMatches(trimmed, tipo, allDrives)
        .then(setPreviewCount)
        .catch(() => setPreviewCount(null));
    }, 220);
    return () => window.clearTimeout(timer);
  }, [patron, tipo, allDrives]);

  const handlePickFolder = () => {
    api
      .devPickProjectRoot()
      .then((picked) => {
        if (picked) {
          setPatron(picked);
          setTipo("carpeta-recursiva");
        }
      })
      .catch((e) => onError(String(e)));
  };

  const handlePickFile = () => {
    api
      .pickFile()
      .then((picked) => {
        if (picked) {
          setPatron(picked);
          setTipo("archivo");
        }
      })
      .catch((e) => onError(String(e)));
  };

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault();
    if (!patron.trim()) return;
    api
      .protectionAdd(patron.trim(), tipo, motivo.trim(), allDrives)
      .then(() => {
        setPatron("");
        setMotivo("");
        setAllDrives(false);
        setPreviewCount(null);
        onAdded();
      })
      .catch((err) => onError(String(err)));
  };

  const handleTest = (e: React.FormEvent) => {
    e.preventDefault();
    if (!testPath.trim()) return;
    api
      .protectionCheck(testPath.trim())
      .then(setTestResult)
      .catch((err) => onError(String(err)));
  };

  return (
    <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
      <Card>
        <h2 className="m-0 text-[14px] font-semibold text-fg-primary mb-2.5">
          Agregar ruta o patrón protegido
        </h2>
        <form onSubmit={handleAdd} className="flex flex-col gap-2.5">
          <div className="flex gap-2 flex-wrap">
            <Button size="sm" onClick={handlePickFolder}>
              <FolderPlus size={13} />
              Carpeta…
            </Button>
            <Button size="sm" onClick={handlePickFile}>
              <FilePlus2 size={13} />
              Archivo…
            </Button>
            <select
              className="h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default text-[12px] text-fg-primary focus:border-accent focus:outline-none"
              value={tipo}
              onChange={(e) => setTipo(e.target.value as ProtectionType)}
            >
              <option value="carpeta-recursiva">Carpeta recursiva</option>
              <option value="archivo">Archivo exacto</option>
              <option value="glob">Patrón glob (*)</option>
            </select>
          </div>

          <input
            type="text"
            className="h-9 px-3 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus:outline-none"
            placeholder="Ej: E:\Proyectos\produccion  o  *.kdbx"
            value={patron}
            onChange={(e) => setPatron(e.target.value)}
          />
          <input
            type="text"
            className="h-9 px-3 rounded-[7px] bg-canvas border border-border-default text-[12.5px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus:outline-none"
            placeholder="Motivo (ej: Bóveda de contraseñas)"
            value={motivo}
            onChange={(e) => setMotivo(e.target.value)}
          />

          <div className="flex items-center justify-between flex-wrap gap-2">
            <label className="flex items-center gap-2 text-[12px] text-fg-secondary cursor-pointer select-none">
              <input
                type="checkbox"
                className="accent-accent"
                checked={allDrives}
                onChange={(e) => setAllDrives(e.target.checked)}
              />
              Aplicar en todas las unidades
            </label>
            <Button type="submit" variant="primary" size="sm" disabled={!patron.trim()}>
              <Plus size={13} />
              Agregar regla
            </Button>
          </div>

          {previewCount !== null && (
            <div className="text-[11.5px] text-accent">
              Coincidencias en el escaneo actual:{" "}
              <strong className="mono">{previewCount.toLocaleString()}</strong> rutas.
            </div>
          )}
        </form>
      </Card>

      <Card>
        <h2 className="m-0 text-[14px] font-semibold text-fg-primary mb-2.5">
          Probar una ruta contra el motor
        </h2>
        <form onSubmit={handleTest} className="flex gap-2 flex-wrap">
          <input
            type="text"
            className="flex-1 min-w-[200px] h-9 px-3 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus:outline-none"
            placeholder="Pega cualquier ruta (ej: C:\Windows\System32\cmd.exe)"
            value={testPath}
            onChange={(e) => setTestPath(e.target.value)}
          />
          <Button type="submit" size="sm" disabled={!testPath.trim()}>
            <SearchCheck size={13} />
            Probar
          </Button>
        </form>
        {testResult && (
          <div className="mt-3">
            <AlertBanner tone={testResult.protected ? "warning" : "success"}>
              <div className="flex items-center gap-2 flex-wrap">
                <Badge tone={testResult.protected ? "warning" : "success"}>
                  {testResult.protected ? "PROTEGIDA" : "NO PROTEGIDA"}
                </Badge>
                <span className="text-[12.5px]">
                  {testResult.protected
                    ? `${testResult.reason ?? "Bloqueada"}${testResult.entry?.patron ? ` (${testResult.entry.patron})` : ""}`
                    : "No coincide con ninguna regla de protección."}
                </span>
              </div>
            </AlertBanner>
          </div>
        )}
      </Card>
    </div>
  );
}
