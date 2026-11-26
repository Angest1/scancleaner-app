import { ChevronDown, FilePlus, FolderPlus, Lock, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Collapse } from "../components/ui/Collapse";
import { AlertBanner } from "../components/ui/Feedback";
import { type ProtectedEntry, type ProtectionType, api } from "../lib/ipc";

export function ProtectedPathsView() {
  const [entries, setEntries] = useState<ProtectedEntry[]>([]);
  const [patron, setPatron] = useState("");
  const [tipo, setTipo] = useState<ProtectionType>("carpeta-recursiva");
  const [motivo, setMotivo] = useState("");
  const [todasLasUnidades, setTodasLasUnidades] = useState(false);
  const [previewCount, setPreviewCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [userOpen, setUserOpen] = useState(true);
  const [criticalOpen, setCriticalOpen] = useState(true);

  const load = useCallback(() => {
    api
      .protectionList()
      .then(setEntries)
      .catch((err) => alert(String(err)));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    const trimmed = patron.trim();
    if (!trimmed) {
      setPreviewCount(null);
      return;
    }
    const timer = setTimeout(() => {
      api
        .protectionPreviewMatches(trimmed, tipo, todasLasUnidades)
        .then(setPreviewCount)
        .catch(() => setPreviewCount(null));
    }, 250);
    return () => clearTimeout(timer);
  }, [patron, tipo, todasLasUnidades]);

  const criticalEntries = useMemo(() => entries.filter((e) => e.origen === "crítica"), [entries]);
  const userEntries = useMemo(() => entries.filter((e) => e.origen === "usuario"), [entries]);

  const handlePickFile = () => {
    api
      .pickFile()
      .then((picked) => {
        if (picked) {
          setPatron(picked);
          setTipo("archivo");
          setTodasLasUnidades(false);
          setError(null);
        }
      })
      .catch((err) => alert(String(err)));
  };

  const handlePickFolder = () => {
    api
      .devPickProjectRoot()
      .then((picked) => {
        if (picked) {
          setPatron(picked);
          setTipo("carpeta-recursiva");
          setTodasLasUnidades(false);
          setError(null);
        }
      })
      .catch((err) => alert(String(err)));
  };

  const handleAdd = () => {
    const trimmed = patron.trim();
    if (!trimmed) {
      setError("Ingresa una ruta o patrón glob válido.");
      return;
    }
    if (tipo === "glob" && !trimmed.includes("*") && !trimmed.includes("?")) {
      setError("Un patrón glob debe contener al menos un comodín (`*` o `?`).");
      return;
    }
    setError(null);
    api
      .protectionAdd(trimmed, tipo, motivo.trim(), todasLasUnidades)
      .then((updated) => {
        setEntries(updated);
        setPatron("");
        setMotivo("");
        setTodasLasUnidades(false);
        setPreviewCount(null);
      })
      .catch((err) => setError(String(err)));
  };

  const handleRemove = (id: string) => {
    api
      .protectionRemove(id)
      .then(setEntries)
      .catch((err) => alert(String(err)));
  };

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
          Rutas protegidas (Lista negra)
        </h1>
        <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
          Las rutas protegidas bloquean el borrado en todas las capas (Treemap, Limpieza segura y
          Limpieza Dev). Las entradas de usuario tienen máxima prioridad incluso sobre excepciones
          del sistema.
        </p>
      </div>

      {/* Add user rule card */}
      <Card className="flex flex-col gap-3">
        <div className="font-semibold text-[14px] text-fg-primary">
          Agregar ruta protegida de usuario
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <Button onClick={handlePickFile}>
            <FilePlus size={13} />
            Elegir archivo...
          </Button>
          <Button onClick={handlePickFolder}>
            <FolderPlus size={13} />
            Elegir carpeta...
          </Button>
          <select
            aria-label="Tipo de protección"
            className="h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default text-[12.5px] text-fg-primary focus:border-accent focus:outline-none"
            value={tipo}
            onChange={(e) => setTipo(e.target.value as ProtectionType)}
          >
            <option value="carpeta-recursiva">Carpeta (recursiva)</option>
            <option value="archivo">Archivo exacto</option>
            <option value="glob">Patrón glob (ej. **\*.bak)</option>
          </select>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <input
            type="text"
            name="protected-pattern"
            autoComplete="off"
            spellCheck={false}
            aria-label="Ruta o patrón glob"
            className="flex-[2] min-w-[240px] h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default mono text-[12px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus-visible:ring-2 focus-visible:ring-accent/30"
            placeholder="Ruta (ej. D:\Trabajo\Secreto) o glob (ej. **\*.pem)"
            value={patron}
            onChange={(e) => {
              setPatron(e.target.value);
              setError(null);
            }}
          />
          <input
            type="text"
            name="protected-reason"
            autoComplete="off"
            aria-label="Motivo opcional"
            className="flex-1 min-w-[170px] h-8 px-2.5 rounded-[7px] bg-canvas border border-border-default text-[12.5px] text-fg-primary placeholder:text-fg-muted focus:border-accent focus-visible:ring-2 focus-visible:ring-accent/30"
            placeholder="Motivo (opcional)"
            value={motivo}
            onChange={(e) => setMotivo(e.target.value)}
          />
          <Button variant="primary" onClick={handleAdd}>
            <Plus size={13} />
            Proteger
          </Button>
        </div>

        <div className="flex items-center justify-between flex-wrap gap-2.5 text-[12.5px]">
          <label className="inline-flex items-center gap-2 text-fg-secondary cursor-pointer select-none">
            <input
              type="checkbox"
              className="accent-accent"
              checked={todasLasUnidades}
              disabled={tipo === "glob"}
              onChange={(e) => setTodasLasUnidades(e.target.checked)}
            />
            Aplicar a todas las unidades (<code className="text-fg-primary">?:\</code>)
          </label>

          {previewCount !== null && (
            <Badge tone="accent">
              Vista previa último escaneo: protegería {previewCount.toLocaleString()} ítems
            </Badge>
          )}
        </div>

        {error && <AlertBanner tone="danger">{error}</AlertBanner>}
      </Card>

      {/* User entries */}
      <Card className="flex flex-col">
        <button
          type="button"
          aria-expanded={userOpen}
          onClick={() => setUserOpen((v) => !v)}
          className="w-full flex items-center justify-between gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
        >
          <div className="flex items-center gap-2">
            <ChevronDown
              size={15}
              className={`text-fg-muted transition-transform duration-200 ease-out ${
                userOpen ? "rotate-0" : "-rotate-90"
              }`}
            />
            <h2 className="m-0 text-[14px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
              Entradas de usuario ({userEntries.length})
            </h2>
          </div>
          <Badge tone="accent">Prioridad 1 (máxima)</Badge>
        </button>

        <Collapse open={userOpen}>
          <div className="pt-3">
            {userEntries.length === 0 ? (
              <div className="text-[12.5px] text-fg-muted py-1">
                Aún no has agregado rutas personalizadas. También puedes hacer clic derecho en el
                Treemap o en el Ranking y elegir &ldquo;Proteger esta ruta&rdquo;.
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {userEntries.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex items-center justify-between gap-3 px-3 py-2.5 rounded-[8px] bg-canvas border border-border-subtle"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <ShieldCheck size={13} className="text-accent shrink-0" />
                        <span className="mono font-medium text-[12.5px] text-fg-primary break-all">
                          {entry.todasLasUnidades ? `?:\\${entry.patron}` : entry.patron}
                        </span>
                        <Badge>{entry.tipo}</Badge>
                      </div>
                      {entry.motivo && (
                        <div className="text-[12px] text-fg-secondary mt-0.5">{entry.motivo}</div>
                      )}
                    </div>
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => handleRemove(entry.id)}
                      title="Quitar entrada de usuario"
                    >
                      <Trash2 size={12} />
                      Quitar
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </Collapse>
      </Card>

      {/* Critical factory entries (read-only) */}
      <Card className="flex flex-col">
        <button
          type="button"
          aria-expanded={criticalOpen}
          onClick={() => setCriticalOpen((v) => !v)}
          className="w-full flex items-center justify-between gap-2 bg-transparent border-0 p-0 cursor-pointer text-left group"
        >
          <div className="flex items-center gap-2">
            <ChevronDown
              size={15}
              className={`text-fg-muted transition-transform duration-200 ease-out ${
                criticalOpen ? "rotate-0" : "-rotate-90"
              }`}
            />
            <h2 className="m-0 text-[14px] font-semibold text-fg-primary group-hover:text-accent transition-colors">
              Entradas críticas de fábrica ({criticalEntries.length})
            </h2>
          </div>
          <Badge tone="warning">Inmutables · Solo lectura</Badge>
        </button>

        <Collapse open={criticalOpen}>
          <div className="flex flex-col gap-1.5 pt-3">
            {criticalEntries.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center justify-between gap-3 px-3 py-2 rounded-[8px] bg-canvas border border-border-subtle"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 flex-wrap">
                    <Lock size={12} className="text-warning shrink-0" />
                    <span className="mono font-medium text-[12px] text-fg-primary break-all">
                      {entry.todasLasUnidades ? `?:\\${entry.patron}` : entry.patron}
                    </span>
                    <Badge>{entry.tipo}</Badge>
                  </div>
                  <div className="text-[12px] text-fg-secondary mt-0.5">{entry.motivo}</div>
                </div>
                <Badge tone="warning" title="Bloqueada de fábrica; no editable">
                  <Lock size={10} />
                  Crítica
                </Badge>
              </div>
            ))}
          </div>
        </Collapse>
      </Card>
    </div>
  );
}
