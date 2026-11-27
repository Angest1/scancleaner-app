# ScanCleaner — Protocolo del Servicio Opcional de Indexación y Modelo de Seguridad

Para ofrecer velocidad de lectura directa de la MFT y sincronización incremental con el USN Change Journal **sin pedir UAC cada vez que se abre la app**, ScanCleaner incluye un servicio opcional de Windows estrictamente de **solo lectura de metadatos**.

---

## 1. Arquitectura de binario único

El propio binario `ScanCleaner.exe` actúa en dos modos:
- **Modo escritorio (normal):** inicia la ventana Tauri v2 como usuario estándar.
- **Modo servicio:** `ScanCleanerIndexService.exe --service --allowed-sid <USER_SID>`.

Al pulsar **Instalar servicio** desde **Ajustes**:
1. Se solicita elevación UAC **una sola vez**.
2. El binario se copia automáticamente a `%ProgramData%\ScanCleaner\ScanCleanerIndexService.exe` (manteniendo `release-portable\ScanCleaner.exe` portable y nunca bloqueado).
3. Se registra el servicio Windows `ScanCleanerIndexService` (`LocalSystem`, inicio automático) pasando el SID exacto del usuario que lo instaló (`--allowed-sid S-1-5-21-...`).
4. Al desinstalar desde **Ajustes**, se detiene el servicio, se elimina del SCM y se borra la copia en `%ProgramData%\ScanCleaner\`.

Si el servicio **no** está instalado, ScanCleaner funciona normalmente con la ruta de respaldo paralela (`FallbackWorkStealing`) y muestra un aviso no intrusivo indicando cómo acelerar los escaneos desde *Ajustes*.

---

## 2. Seguridad del Named Pipe y garantías de solo lectura

- **Nombre del pipe:** `\\.\pipe\ScanCleanerIndexSvc_v1`
- **ACL estricta (SDDL):** el servidor crea el pipe con `ConvertStringSecurityDescriptorToSecurityDescriptorW` usando:
  ```text
  D:(A;;GA;;;SY)(A;;GA;;;<USER_SID>)
  ```
  Esto otorga acceso exclusivamente a `SYSTEM` (`SY`) y al SID del usuario local que instaló el servicio. Ningún otro usuario local o remoto puede conectarse al pipe.
- **Garantía de solo lectura a nivel de protocolo:** el enum `ServiceRequest` (`src-tauri/src/service/protocol.rs`) **no contiene ningún opcode de escritura, borrado, renombrado ni ejecución de procesos**. El servicio solo abre handles de volumen con `FILE_READ_DATA | FILE_READ_ATTRIBUTES`.
- **Validación estricta de frames:**
  - Framing: longitud `u32` little-endian (4 bytes) + payload JSON UTF-8.
  - Tamaño máximo de solicitud: **`4096` bytes** (`MAX_REQUEST_FRAME_BYTES`).
  - Tamaño máximo de respuesta binaria: **`128 MB`** (`MAX_RESPONSE_FRAME_BYTES`).
  - Toda solicitud pasa por `decode_request_frame()` y `validate_request()`: valida letra de unidad ASCII (`A..=Z`), rechaza `start_usn < 0` y rechaza cualquier JSON desconocido o malformado.

---

## 3. Especificación de mensajes (`PROTOCOL_VERSION = 1`)

### Solicitudes (`ServiceRequest`)

| `op` | Campos | Descripción |
|---|---|---|
| `ping` | *(ninguno)* | Comprueba disponibilidad del servicio, versión de protocolo y flag `read_only: true` |
| `query_usn_journal` | `drive_letter: char` | Ejecuta `FSCTL_QUERY_USN_JOURNAL` y devuelve `UsnJournalID`, `FirstUsn`, `NextUsn`, `LowestValidUsn` |
| `read_usn_records` | `drive_letter: char`, `journal_id: u64`, `start_usn: i64` | Ejecuta `FSCTL_READ_USN_JOURNAL` y devuelve lista de `UsnEvent` + `next_usn` |
| `scan_mft` | `drive_letter: char`, `include_ads: bool` | Escanea la `$MFT` del volumen en solo lectura, serializa un snapshot `SCIDX001` comprimido con LZ4 en memoria/temp y lo transmite como frame binario tras la cabecera `MftSnapshotReady` |

### Respuestas (`ServiceResponse`)

| `status` | Campos | Descripción |
|---|---|---|
| `pong` | `protocol_version`, `read_only` | Confirmación de salud del servicio |
| `usn_journal` | `info: UsnJournalInfo` | Metadatos actuales del USN Change Journal |
| `usn_records` | `events: Vec<UsnEvent>`, `next_usn: i64` | Deltas USN desde `start_usn` |
| `mft_snapshot_ready` | `payload_bytes`, `usn_journal_id`, `last_usn` | Precede a un frame binario de `payload_bytes` con el snapshot `SCIDX001` |
| `error` | `code`, `message` | Error estructurado sin cierre abrupto del servidor |

---

## 4. Preservación de seguridad en borrado (`isProtected()`)

El servicio de indexación nunca ejecuta borrados. Todas las operaciones destructivas siguen corriendo en el contexto del usuario dentro de la app principal, manteniendo intactas todas las capas de seguridad existentes:
1. Lista negra crítica e inmutable (`ProtectionEngine::is_protected()`), verificada **justo antes de cada rename a `.scancleaner-trash`** y **antes de cada borrado**.
2. Validación de reparse points (`is_reparse_point`) para impedir traversal hacia rutas protegidas vía junctions/symlinks.
3. Vista previa **Dry-run obligatoria** antes de confirmar cualquier limpieza.
4. Registro persistente JSONL de operaciones y resultados (incluyendo espacio estimado vs. real liberado).
5. Cero telemetría por defecto; únicamente logs locales de diagnóstico con niveles (`%LOCALAPPDATA%\ScanCleaner\logs\`).
