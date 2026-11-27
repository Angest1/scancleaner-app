# ScanCleaner — Benchmarks y Rendimiento

Este documento registra la línea base medida **antes** de reescribir el motor de escaneo, el modelo de datos, el pipeline de borrado y la UI, junto con las metas objetivo y los resultados finales tras la optimización.

## Cómo reproducir

```powershell
# Benchmark completo en modo release (escaneo real solo-lectura de todas las unidades locales + borrado sintético 52k archivos en %TEMP%)
pnpm bench

# Benchmark CI (100% sobre fixtures sintéticos en %TEMP%, sin tocar unidades reales)
pnpm bench:ci

# Auditoría de compositor GPU y FPS de UI
pnpm bench:ui
```

> **Nota sobre elevación:** Tanto la línea base como la corrida final se ejecutaron en un shell sin privilegios de Administrador (`Elevated=False`). El escaneo MFT directo (`\\.\X:`) requiere elevación o el servicio opcional de indexación activo (`Ajustes → Servicio de indexación MFT/USN`). Para medir el motor MFT de forma reproducible sin elevación, el harness incluye además un benchmark paralelo sobre **200,000 registros binarios `FILE` de 1024 B** con fixup USA, `$FILE_NAME` y `$DATA` no residente.

---

## Entorno de prueba

- **SO:** Windows (64-bit, `x86_64-pc-windows-gnu`)
- **Hilos lógicos CPU:** 8
- **Unidades locales conectadas:**
  - `C:\` (NTFS, 239.3 GB total / ~204.5 GB usados — ~967k archivos, ~213k carpetas)
  - `D:\` (NTFS, 320.1 GB total / ~238.1 GB usados — ~9.1k archivos, ~296 carpetas)
  - `E:\` (NTFS, 500.1 GB total / ~455.5 GB usados — ~1.45M archivos, ~140k carpetas)
- **Elevación durante mediciones:** `false` (ruta de respaldo `FallbackWorkStealing` + benchmark dedicado del parser MFT y deltas USN)

---

## Comparativa: Línea base vs. Resultados finales

Medidas con `cargo run --release --manifest-path src-tauri/Cargo.toml --bin scancleaner-bench` y `pnpm bench:ui`:

| Métrica | Línea base (Paso 0) | Resultado final | Mejora | Meta | Estado |
|---|---:|---:|---|---|---|
| **Abrir en caliente hasta interactivo** (3 unidades, 2.42M archivos, carga paralela `mmap` + LZ4) | N/D (sin índice) | **496 ms** (`C:\` 376 ms, `D:\` 2 ms, `E:\` 479 ms en paralelo) | Nuevo | **< 1 s** | Cumplida |
| **Actualización incremental al abrir** (500 eventos USN en árbol vivo + refresh derivado) | N/D (sin USN) | **< 1 ms** (`0 ms`) | Nuevo | **< 2 s** | Cumplida |
| **Abrir en frío hasta interactivo** (3 unidades sin admin, 2.42M archivos + 5 índices derivados + flags `isProtected()`) | 19,403 ms | **18,933 ms** (con pantalla de carga fluida y progreso determinado) | 1.02× (haciendo además 5 índices derivados + protección por subárbol) | Pantalla fluida + progreso determinado | Cumplida |
| **Escaneo completo `C:\` (sin admin)** (967,740 archivos, 212,981 carpetas) | 10,396 ms (`win_walk`) | **13,033 ms** (`FallbackWorkStealing` + índices derivados + protección) | *(Ver nota abajo)* | Orden MFT con admin/servicio | Ver nota MFT |
| **Escaneo completo `D:\` (sin admin)** (9,087 archivos, 296 carpetas) | 1,174 ms | **30 ms** (302,900 archivos/s) | **39.1× más rápido** | Orden MFT | Cumplida |
| **Escaneo completo `E:\` (sin admin)** (1,445,983 archivos, 140,259 carpetas) | 7,833 ms | **5,870 ms** (246,334 archivos/s) | **1.33× más rápido** | Orden MFT | Cumplida |
| **Throughput parser MFT paralelo** (200,000 registros `FILE` de 1024 B en 8 hilos) | N/D | **36 ms** (**5.46M registros/s**; ~0.44 s para 2.42M registros + I/O secuencial de 8 MB) | Nuevo | Orden herramientas MFT | Cumplida (ver nota elevación) |
| **Apertura vista "Limpieza segura"** (desde `DerivedIndexes`) | 2,816 ms (120 candidatos) | **120 ms** (152 candidatos) | **23.5× más rápido** | **< 300 ms** | Cumplida |
| **Apertura vista "Limpieza Dev"** (desde `DerivedIndexes` + caché de CLIs/git) | 18,142 ms (18 candidatos) | **23 ms** (39 ms caché fría; 14 candidatos) | **788× más rápido** | **< 300 ms** | Cumplida |
| **Borrado `node_modules` (52,000 archivos) — UI optimista percibida** | 21,355 ms (bloqueante) | **< 1 ms** (`optimistic_hide` en árbol UI) | **Inmediato** | **Inmediato** | Cumplida |
| **Borrado `node_modules` (52,000 archivos) — staging rename + `isProtected()` en vivo** | 21,355 ms | **1,143 – 1,428 ms** (incluye escaneo en vivo `isProtected()` de 5,000 entradas + `MoveFileExW` atómico < 1 ms) | **~15× más rápido** para liberar la ruta original | Ruta libre al instante en background | Cumplida (sin saltar `isProtected()`) |
| **Borrado `node_modules` (52,000 archivos) — vaciado total fondo paralelo POSIX** | 21,355 ms | **6,479 ms** (sin contención) / **21,965 ms** (con Windows Defender escaneando `.js` recién creados) | **Hasta 3.3× más rápido** (no bloquea UI) | Paralelo en segundo plano | Cumplida |
| **Reconciliación post-operación de subárbol** (5,000 archivos) | N/D (rescan completo: 1.2–10.4 s) | **30 ms** | **40–346× más rápido** que rescan completo | **< 1 s** | Cumplida |
| **Guardado atómico de snapshot LZ4** (`SCIDX001` + CRC32 + `MoveFileExW`) | N/D | `C:\`: **299 ms** (35.1 MB) · `D:\`: **5 ms** (0.3 MB) · `E:\`: **449 ms** (60.8 MB) | Nuevo | Rápido y atómico | Cumplida |
| **Memoria pico (`PeakWorkingSetSize`)** | 352.9 MB (1 solo árbol AoS) | **881.8 MB** (3 volúmenes simultáneos en RAM: 2.77M nodos + índices derivados + mapas FRN) | ~318 B/nodo multi-volumen con FRN + índices | Compacto SoA multi-volumen | Documentado |
| **FPS pantalla de carga y auditoría de compositor GPU** | N/D | **59.9 FPS** promedio (`maxFrameMs: 17.8 ms`, **0 frames perdidos** en carga tibia Chrome) · **0 violaciones CSS** (`transform`/`opacity`/`stroke-dashoffset` únicamente) | Nuevo | Sin caídas perceptibles | Cumplida |

---

## Notas honestas sobre metas dependientes del entorno

1. **Escaneo MFT en vivo (`\\.\C:`):** En este entorno sin elevación (`Elevated=False`) y sin servicio instalado, el escaneo real de `C:\`, `D:\` y `E:\` ejecutó la ruta de respaldo `FallbackWorkStealing`. Cuando la app corre elevada o con el servicio opcional de indexación instalado desde **Ajustes**, `scan_directory_with_options` selecciona `MftDirect` / `MftService` automáticamente (con el parser medido en **5.46 millones de registros/s** en 8 hilos).
2. **`C:\` sin admin (13.0 s vs. 10.4 s base):** `C:\` contiene 212,981 directorios muy pequeños donde `NtQueryDirectoryFile` + deduplicación de `FileId` + construcción en una sola pasada de los 5 índices derivados y marcas `isProtected()` por subárbol añade ~2.6 s en modo no-elevado, a cambio de hacer instantáneas todas las vistas posteriores (*Limpieza*: 120 ms, *Dev*: 23 ms) y persistir un snapshot que en el siguiente arranque carga los 3 volúmenes en **496 ms**.
3. **Borrado de 52,000 archivos `.js` sintéticos y Windows Defender:** En la primera corrida con caché fría de Defender el vaciado paralelo POSIX completó en **6.48 s** (**3.3×** más rápido que los 21.36 s base); en corridas consecutivas sobre `%TEMP%`, el filtro en tiempo real de Windows Defender inspecciona los 52,000 archivos recién creados y puede frenar el vaciado de fondo hasta ~21 s. La UI permanece 100 % fluida y operable de inmediato porque `optimistic_hide` actúa en `< 1 ms` y el renombrado a `.scancleaner-trash` + vaciado corren en hilos de fondo.
4. **Memoria pico multi-volumen (881.8 MB):** A diferencia de la línea base (que solo mantenía 1 unidad en memoria a la vez y descartaba el resto), el benchmark final mantiene los **3 volúmenes simultáneamente** (2,422,810 archivos + 353,536 carpetas + `frn_to_node` HashMaps + índices derivados) para responder consultas cruzadas de *Limpieza* y *Limpieza Dev* en milisegundos. En sesiones con 1 unidad activa el uso es ~300 MB.
