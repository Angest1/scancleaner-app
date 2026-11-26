# ScanCleaner

Aplicación de escritorio ligera para Windows 10/11 construida con **Tauri v2 (Rust + React/TypeScript)** para analizar el espacio en disco, visualizar jerarquías y categorías, inspeccionar aplicaciones instaladas y liberar espacio de forma segura.

## Características principales

- **Escaneo rápido y seguro de unidades**:
  - Unidades fijas y removibles al instante; unidades de red bajo demanda.
  - Enumeración paralela NTFS con `NtQueryDirectoryFile` (`FileIdBothDirectoryInformation`) y fallback automático a `FindFirstFileExW` en FAT32/exFAT/red.
  - Deduplicación de hardlinks por `FileId`, exclusión de ciclos en symlinks/junctions, reporte de rutas sin acceso (`AccessDenied`) y contabilidad por tamaño asignado en disco (compatible con archivos *sparse*, comprimidos y placeholders de OneDrive).
- **Visualización interactiva**:
  - **Treemap squarified** en Canvas 2D con drill-down, breadcrumb, tooltips y menú contextual.
  - **Ranking** virtualizado de carpetas y archivos más grandes.
  - **Categorización** por tipo de archivo y por origen, personalizable mediante `%APPDATA%\ScanCleaner\rules.json`.
- **Aplicaciones instaladas**:
  - Listado unificado Win32 (Registro `HKLM` / `WOW6432Node` / `HKCU`) y Microsoft Store / MSIX (`PackageManager`).
  - Medición conservadora de tamaño real vs estimado.
  - Desinstalación **exclusivamente nativa** (`UninstallString` o `ms-settings:appsfeatures`).
- **Limpieza con salvaguardas**:
  - Temporales de usuario y sistema, Papelera, cachés de navegadores (con detección de procesos activos), artefactos de desarrollo (`node_modules`, `vendor`, `.venv`, `target`, `bin`/`obj`, cachés) y archivos grandes/antiguos.
  - *Dry-run* obligatorio, confirmación explícita, envío a la Papelera por defecto (`IFileOperation`, sin degradación silenciosa a borrado permanente), lista negra inmutable de rutas del sistema y log auditable JSONL.
- **Elevación UAC bajo demanda e historial**:
  - Ejecución normal (`asInvoker`) con botón para reiniciar como administrador.
  - Snapshots agregados en SQLite para seguir la evolución del espacio libre/usado.

## Desarrollo

### Prerrequisitos (Windows 10/11)
- Node.js 22+
- Rust (`stable-x86_64-pc-windows-msvc`, con `rustfmt` y `clippy`)
- Visual Studio Build Tools 2022 (`VCTools` + Windows 10/11 SDK) para toolchain MSVC, o bien toolchain GNU (`stable-x86_64-pc-windows-gnu` + `BrechtSanders.WinLibs.POSIX.MSVCRT`) en espacio de usuario sin elevación UAC
- Microsoft Edge WebView2 Runtime (incluido en Windows 11 y Windows 10 actualizado)

### Comandos
```powershell
npm install
npm run dev           # Frontend Vite
npm run tauri dev     # App completa Tauri
npm run lint          # Biome check
npm run typecheck     # TypeScript
npm test              # Vitest
```

### Verificación completa y tests en sandbox
```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Los tests de integración de escaneo y limpieza (`src-tauri/tests/sandbox_fixtures.rs`) crean fixtures temporales aislados (`TempDir`) con hardlinks, archivos *sparse* y junctions cíclicos protegidos por `SandboxGuard`, sin tocar archivos reales ni la Papelera del sistema.

## Distribución, SmartScreen y firma de código

- **Instalador NSIS y build Portable**: `npm run tauri build` genera el instalador NSIS en `src-tauri/target/release/bundle/nsis/` y el ejecutable autocontenido en `src-tauri/target/release/scancleaner.exe` (empaquetable en `.zip` portable).
- **Windows Defender SmartScreen**: las compilaciones locales o sin firma de código mostrarán la advertencia estándar de SmartScreen (*"Windows protegió su PC"*). Para distribución pública se recomienda firmar el binario y el instalador mediante **Azure Trusted Signing** en el workflow de release.
