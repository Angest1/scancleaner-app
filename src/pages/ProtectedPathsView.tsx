import { FilePlus, FolderPlus, Lock, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type ProtectedEntry, type ProtectionType, api } from "../lib/ipc";

export function ProtectedPathsView() {
  const [entries, setEntries] = useState<ProtectedEntry[]>([]);
  const [patron, setPatron] = useState("");
  const [tipo, setTipo] = useState<ProtectionType>("carpeta-recursiva");
  const [motivo, setMotivo] = useState("");
  const [todasLasUnidades, setTodasLasUnidades] = useState(false);
  const [previewCount, setPreviewCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

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
    <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
      <div>
        <h2 style={{ margin: 0, fontSize: 20 }}>Rutas protegidas (Lista negra)</h2>
        <p style={{ margin: "4px 0 0", color: "var(--text-secondary)", fontSize: 13 }}>
          Las rutas protegidas bloquean el borrado en todas las capas (Treemap, Limpieza segura y
          Limpieza Dev). Las entradas de usuario tienen máxima prioridad incluso sobre excepciones
          del sistema.
        </p>
      </div>

      {/* Add user rule card */}
      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <div style={{ fontWeight: 700, fontSize: 15 }}>Agregar ruta protegida de usuario</div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button type="button" className="btn" onClick={handlePickFile}>
            <FilePlus size={14} />
            Elegir archivo...
          </button>
          <button type="button" className="btn" onClick={handlePickFolder}>
            <FolderPlus size={14} />
            Elegir carpeta...
          </button>
          <select
            className="select"
            value={tipo}
            onChange={(e) => setTipo(e.target.value as ProtectionType)}
          >
            <option value="carpeta-recursiva">Carpeta (recursiva)</option>
            <option value="archivo">Archivo exacto</option>
            <option value="glob">Patrón glob (ej. **\*.bak)</option>
          </select>
        </div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <input
            type="text"
            className="input mono"
            style={{ flex: 2, minWidth: 260 }}
            placeholder="Ruta (ej. D:\Trabajo\Secreto) o glob (ej. **\*.pem)"
            value={patron}
            onChange={(e) => {
              setPatron(e.target.value);
              setError(null);
            }}
          />
          <input
            type="text"
            className="input"
            style={{ flex: 1, minWidth: 180 }}
            placeholder="Motivo (opcional)"
            value={motivo}
            onChange={(e) => setMotivo(e.target.value)}
          />
          <button type="button" className="btn btn-primary" onClick={handleAdd}>
            <Plus size={14} />
            Proteger
          </button>
        </div>

        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 10,
            fontSize: 12.5,
          }}
        >
          <label style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={todasLasUnidades}
              disabled={tipo === "glob"}
              onChange={(e) => setTodasLasUnidades(e.target.checked)}
            />
            Aplicar a todas las unidades (<code>?:\</code>)
          </label>

          {previewCount !== null && (
            <span className="badge badge-info">
              Vista previa último escaneo: protegería {previewCount.toLocaleString()} ítems
            </span>
          )}
        </div>

        {error && <div style={{ fontSize: 12.5, color: "#fca5a5" }}>{error}</div>}
      </div>

      {/* User entries */}
      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3 style={{ margin: 0, fontSize: 15 }}>Entradas de usuario ({userEntries.length})</h3>
          <span className="badge badge-info">Prioridad 1 (máxima)</span>
        </div>

        {userEntries.length === 0 ? (
          <div style={{ color: "var(--text-muted)", fontSize: 13 }}>
            Aún no has agregado rutas personalizadas. También puedes hacer clic derecho en el
            Treemap o en el Ranking y elegir &ldquo;Proteger esta ruta&rdquo;.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {userEntries.map((entry) => (
              <div
                key={entry.id}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  gap: 12,
                  padding: "9px 12px",
                  borderRadius: 8,
                  background: "#0f172a",
                  border: "1px solid var(--border)",
                }}
              >
                <div style={{ overflow: "hidden" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                    <ShieldCheck size={14} color="#38bdf8" />
                    <span className="mono" style={{ fontWeight: 600, fontSize: 13 }}>
                      {entry.todasLasUnidades ? `?:\\${entry.patron}` : entry.patron}
                    </span>
                    <span className="badge">{entry.tipo}</span>
                  </div>
                  <div style={{ fontSize: 12, color: "var(--text-secondary)", marginTop: 2 }}>
                    {entry.motivo}
                  </div>
                </div>
                <button
                  type="button"
                  className="btn btn-danger"
                  style={{ padding: "5px 9px" }}
                  onClick={() => handleRemove(entry.id)}
                  title="Quitar entrada de usuario"
                >
                  <Trash2 size={13} />
                  Quitar
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Critical factory entries (read-only) */}
      <div className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3 style={{ margin: 0, fontSize: 15 }}>
            Entradas críticas de fábrica ({criticalEntries.length})
          </h3>
          <span className="badge badge-warning">Inmutables · Solo lectura</span>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {criticalEntries.map((entry) => (
            <div
              key={entry.id}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                gap: 12,
                padding: "8px 12px",
                borderRadius: 8,
                background: "#0f172a",
                border: "1px solid var(--border)",
              }}
            >
              <div style={{ overflow: "hidden" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
                  <Lock size={13} color="#f59e0b" />
                  <span className="mono" style={{ fontWeight: 600, fontSize: 12.5 }}>
                    {entry.todasLasUnidades ? `?:\\${entry.patron}` : entry.patron}
                  </span>
                  <span className="badge">{entry.tipo}</span>
                </div>
                <div style={{ fontSize: 12, color: "var(--text-secondary)", marginTop: 2 }}>
                  {entry.motivo}
                </div>
              </div>
              <span className="badge badge-warning" title="Bloqueada de fábrica; no editable">
                <Lock size={11} style={{ marginRight: 4 }} />
                Crítica
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
