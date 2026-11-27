# ScanCleaner — Formato Binario de Índice Persistente (`SCIDX001`)

Los snapshots por volumen se persisten en `%LOCALAPPDATA%\ScanCleaner\index\` bajo el nombre determinístico:

```text
vol_<DRIVE_TAG>_<VOLUME_SERIAL_HEX8>.scidx
```

Ejemplo: `vol_C_8A3F120C.scidx`.

---

## 1. Diseño del archivo binario

Cada archivo `.scidx` consiste en una **cabecera fija de 64 bytes** sin comprimir seguida del **payload comprimido con LZ4** (`lz4_flex::compress`):

| Offset | Tamaño | Tipo (LE) | Campo | Descripción |
|---:|---:|---|---|---|
| `0` | 8 | `[u8; 8]` | `magic` | Constante ASCII `"SCIDX001"` |
| `8` | 4 | `u32` | `version` | Versión del formato (`1`). Versiones incompatibles disparan reescaneo limpio |
| `12` | 4 | `u32` | `volume_serial` | Número de serie del volumen Windows (`GetVolumeInformationW`) |
| `16` | 8 | `u64` | `usn_journal_id` | `UsnJournalID` NTFS activo al momento del snapshot (`0` si no aplica) |
| `24` | 8 | `i64` | `last_usn` | Último USN procesado sobre el volumen |
| `32` | 8 | `i64` | `created_ts` | Timestamp Unix UTC de creación del snapshot |
| `40` | 4 | `u32` | `flags` | Bit 0: `include_ads` |
| `44` | 4 | `u32` | `fs_type` | `FileSystemType` (`0=NTFS, 1=FAT32, 2=exFAT, 3=ReFS, 4=Network, 5=Unknown`) |
| `48` | 8 | `u64` | `uncompressed_len` | Longitud exacta en bytes del payload descomprimido |
| `56` | 4 | `u32` | `crc32` | Checksum **CRC32** (`crc32fast`) calculado sobre el payload comprimido (`bytes[64..]`) |
| `60` | 4 | `[u8; 4]` | `reserved` | Reservado (`0`) |
| `64` | `N` | `[u8]` | `lz4_payload` | Payload SoA comprimido con LZ4 |

---

## 2. Estructura del payload descomprimido

El payload descomprimido serializa en bloque las columnas planas de `ScanTree` para minimizar asignaciones y permitir carga rápida tras `memmap2::Mmap`:

1. `node_count: u32`
2. Columnas paralelas contiguas (`node_count` elementos cada una, little-endian):
   - `parent: [u32]`
   - `first_child: [u32]`
   - `next_sibling: [u32]`
   - `name_offset: [u32]`
   - `name_len: [u16]`
   - `logical_size: [u64]`
   - `allocated_size: [u64]`
   - `file_count: [u32]`
   - `last_write_ts: [u32]`
   - `kind: [u8]`
   - `flags: [u16]`
   - `type_cat: [u8]`
   - `origin_cat: [u8]`
   - `file_ref: [u64]`
3. `name_pool_len: u32` seguido de `name_pool: [u8; name_pool_len]`
4. `tail_meta_len: u32` seguido de JSON compacto (`root_path`, `ScanStats`, `DerivedIndexes`, `access_denied`)

Al cargar (`load_volume_snapshot`), el archivo se mapea en memoria con **`memmap2::Mmap`**, se validan `magic`, `version`, `volume_serial` y `crc32` antes de descomprimir, y se reconstruye `frn_to_node` en una sola pasada lineal.

---

## 3. Escritura atómica y seguridad ante cortes

Para garantizar que un cierre inesperado o corte de luz nunca corrompa un índice previo:
1. Se escribe cabecera + payload LZ4 en un archivo temporal hermano `.tmp-<pid>-<uuid>.scidx`.
2. Se ejecuta `File::sync_data()` para asegurar persistencia en disco.
3. Se reemplaza el archivo destino de forma atómica con `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`.

---

## 4. Ciclo de vida al arrancar

1. **Enumeración instantánea de unidades:** `GetDiskFreeSpaceExW` entrega al instante espacio total, usado y libre.
2. **Carga en caliente (`warm start`):** si existe un `.scidx` válido con el mismo `volume_serial`, se carga vía `mmap` y la UI queda operable en < 1 s.
3. **Sincronización incremental USN:**
   - Si NTFS + acceso al journal (admin o servicio): consulta `FSCTL_QUERY_USN_JOURNAL`.
   - Si `UsnJournalID` coincide y `saved_last_usn >= FirstUsn`: aplica deltas (`FSCTL_READ_USN_JOURNAL`) en memoria y propaga tamaños hacia arriba.
   - Si `UsnJournalID` cambió o el journal fue truncado (`saved_last_usn < FirstUsn`): mantiene el snapshot visible en la UI mientras lanza un reescaneo completo en segundo plano.
   - Sin admin ni servicio: marca `stats.stale = true` y refresca en segundo plano con la ruta de respaldo.
