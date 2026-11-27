# ScanCleaner — Sistema de Reconciliación Automática

ScanCleaner mantiene el árbol de archivos, el espacio libre real y todas las vistas (*Inicio*, *Explorador*, *Limpieza*, *Limpieza Dev*, *Apps*) sincronizados automáticamente sin requerir que el usuario actualice manualmente.

---

## 1. Cola central de reconciliación (`src-tauri/src/reconcile/queue.rs`)

Todas las señales de cambio —internas y externas— pasan por una única cola por volumen (`ReconciliationQueue`):

- **Coalescencia de rutas (`coalesce_paths`):** si llegan simultáneamente `C:\a`, `C:\a\b` y `C:\a\b\c`, se colapsan al ancestro común mínimo (`C:\a`) para evitar trabajo duplicado.
- **Debounce de 250 ms:** absorbe ráfagas de escritura (por ejemplo, `npm install`, compilaciones o borrados masivos) en un solo lote por volumen.
- **Prioridad estricta:**
  1. `High`: operaciones iniciadas por el usuario (borrado propio, comando oficial de limpieza, desinstalación nativa, vaciado de Papelera, ajuste de `hiberfil.sys` / `pagefile.sys`).
  2. `Medium`: eventos de foco de ventana (`window-focused`), deriva de espacio libre (`GetDiskFreeSpaceExW`) y hotplug de unidades (`DeviceChange`).
  3. `Low`: eventos continuos de USN Change Journal y `ReadDirectoryChangesW`.
- **Prioridad de I/O reducida:** los lotes `Medium` y `Low` se ejecutan dentro de un guardia RAII `BackgroundIoGuard` que aplica `SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN)` y restaura `THREAD_MODE_BACKGROUND_END` al terminar.
- **Reintentos con backoff exponencial:** si una carpeta está temporalmente bloqueada por otro proceso, el lote se reencola con backoff de `1 s, 2 s, 4 s, ...` hasta un máximo de `60 s`, y reporta el estado en el indicador de sincronización sin bloquear la UI.
- **Prevención estricta de bucles:** el watcher y `coalesce_paths` ignoran de raíz cualquier cambio dentro de `%LOCALAPPDATA%\ScanCleaner\index\`, impidiendo que guardar un snapshot `.scidx` dispare otra reconciliación.

---

## 2. Catálogo completo de disparadores (`src-tauri/src/reconcile/triggers.rs`)

| # | Disparador | Mecanismo | Prioridad | Alcance de actualización |
|---:|---|---|---|---|
| 1 | **Borrado propio** (permanente o Papelera) | Post-ejecución de `cleaner` / `dev_cleaner` + fin de vaciado `.scancleaner-trash` | `High` | Carpetas padre afectadas (+ `$Recycle.Bin` si fue a Papelera) + medición `GetDiskFreeSpaceExW` real vs. estimado |
| 2 | **Comandos oficiales de limpieza** (`npm cache clean`, `docker builder prune`, `sdkmanager`, etc.) | Campo opcional `rutasAfectadas` en `dev_cleaner/rules.json` + invalidación de caché de herramienta | `High` | Subárboles declarados en `rutasAfectadas` + re-chequeo de espacio libre real (con aviso `.vhdx` para Docker/WSL) |
| 3 | **Desinstalación nativa desde la app** (Win32 / MSIX) | Vigila la desaparición real de la clave `Uninstall` en Registro (`RegNotifyChangeKeyValue`) o paquete en `PackageManager` con timeout configurable (10 min por defecto); no confía solo en la salida del primer proceso porque muchos instaladores se relanzan desde `%TEMP%` | `High` | Reconcilia `InstallLocation` + carpetas AppData asociadas, refresca *Apps* y sugiere restos no protegidos **solo en nivel `revisar`** |
| 4 | **Vaciar Papelera** | Post-`SHEmptyRecycleBinW` | `High` | Reconcilia `$Recycle.Bin` en todos los volúmenes locales |
| 5 | **Ajuste de `hiberfil.sys` / `pagefile.sys`** | Post-`powercfg` o regreso de diálogo de Memoria Virtual | `High` | Reconcilia la raíz del volumen de sistema y notifica si requiere reinicio de Windows |
| 6 | **Cambios externos continuos (NTFS)** | Lectura incremental periódica del **USN Change Journal** (directo o vía servicio) | `Low` | Deltas directos en `ScanTree` y `DerivedIndexes` |
| 7 | **Cambios externos continuos (otros FS / sin admin)** | `ReadDirectoryChangesW` por volumen (excluyendo `%LOCALAPPDATA%\ScanCleaner\index`) | `Low` | Subárboles modificados vía `reconcile_subtree_fallback` |
| 8 | **Al recuperar foco de ventana** | Evento Tauri `WindowEvent::Focused(true)` → `cmd_trigger_focus_sync` | `Medium` | Vacía pendientes, comprueba USN + `GetDiskFreeSpaceExW` + tamaño actual de `pagefile.sys`/`hiberfil.sys` |
| 9 | **Verificación periódica de espacio libre** | Cada 60 s compara `GetDiskFreeSpaceExW` contra el total indexado; dispara si la diferencia supera `max(1%, 1 GiB)` (configurable) | `Medium` | Sincronización incremental o reescaneo en segundo plano del volumen con deriva |
| 10 | **Conexión / desconexión de unidades** | Monitor de unidades / `decode_device_unitmask` (`DBT_DEVICEARRIVAL` / `DBT_DEVICEREMOVECOMPLETE`) | `Medium` | Indexa automáticamente unidades nuevas; al desconectar quita la unidad de la vista **conservando su snapshot `.scidx`** para carga instantánea al reconectar |

---

## 3. Reglas de UX en vivo

- **Actualización reactiva y silenciosa:** el indicador `SyncStatusPill` en la barra superior muestra `Sincronizando…` solo mientras hay lotes activos y `Actualizado hace X s/min` en reposo, con opción de clic para sincronizar ahora.
- **Sin saltos bajo el cursor (`useDeferredReorder`):** mientras el puntero interactúa sobre tablas/listas virtualizadas (*Explorador*, *Apps*), los valores (tamaños) se actualizan en el sitio y cualquier reordenamiento se difiere hasta que el puntero sale o pasa 1 segundo de inactividad.
- **Carpeta eliminada externamente:** si el usuario está dentro de una carpeta en el *Explorador* que desaparece por un cambio externo, `cmd_resolve_valid_node` navega al ancestro existente más cercano y muestra un aviso discreto (`<output aria-live="polite">`).
- **Espacio real vs. estimado:** tras borrados y comandos oficiales se registra y muestra el delta real de `GetDiskFreeSpaceExW` junto con una explicación automática si difiere del estimado (`.vhdx` sin compactar, hardlinks compartidos, archivos bloqueados o vaciado de `.scancleaner-trash` aún en curso).
