---
trigger: always_on
---

# Regla de proyecto: actualizar `release-portable`

El usuario ejecuta la aplicación directamente desde `release-portable\ScanCleaner.exe` (`E:\Proyectos\scancleaner-app\release-portable`).

Después de implementar y verificar cualquier cambio en el frontend (`src/`) o backend (`src-tauri/`):
1. Compila el binario release con el frontend embebido ejecutando `npm run tauri -- build --no-bundle` (asegurando `%USERPROFILE%\.cargo\bin` en `PATH` si hace falta).
2. Copia `src-tauri\target\release\scancleaner.exe` a `release-portable\ScanCleaner.exe` (y `WebView2Loader.dll` si existe en `src-tauri\target\release\`).
3. Si `ScanCleaner.exe` está en ejecución y bloquea la copia, avisa al usuario para que lo cierre y repite la copia.
4. No cierres la tarea sin confirmar que `release-portable\ScanCleaner.exe` quedó actualizado con la fecha/hora reciente.
