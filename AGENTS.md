# AGENTS.md — ScanCleaner

Guía operativa para agentes en este repositorio (**ScanCleaner**, app de escritorio Windows 10/11 en Tauri v2 + Rust + React 19 + TypeScript + Tailwind CSS v4).

## Stack de Design Skills instalado (`.agents/skills/`)

- `frontend-design` (`anthropics/skills`): dirección estética base y decisiones visuales intencionales.
- `emil-design-eng`, `animate`, `animation-performance`, `review-animations` (`emilkowalski/skills`): micro-interacciones, curvas de easing, rendimiento de animación y detalles de craft.
- `impeccable` (`pbakaus/impeccable`): auditoría visual, crítica, pulido, endurecimiento y eliminación de *AI slop*.
- `web-design-guidelines` (`vercel-labs/agent-skills`): checklist técnico de accesibilidad, teclado, foco, semántica HTML y producción.

## Jerarquía de resolución de conflictos

Si dos skills o criterios contradicen una regla, prevalece estrictamente este orden:

1. **`DESIGN.md` del proyecto** (autoridad máxima de tokens, superficies, tipografía y layout)
2. **`impeccable`** (anti-slop, auditoría visual, contraste y endurecimiento)
3. **`emilkowalski/skills`** (`emil-design-eng`, `animate`, `animation-performance` — motion y detalle táctil)
4. **`frontend-design`** (dirección estética base)
5. **`web-design-guidelines`** (accesibilidad, foco, semántica y checklist técnico)

## Reglas de ingeniería UI

- **Cero regresiones funcionales:** No modificar contratos IPC (`src/lib/ipc.ts`), lógica de backend Rust (`src-tauri/`) ni algoritmos de layout (`src/lib/treemapLayout.ts`) durante tareas de UI.
- **Modularidad estricta (SRP):** Ningún archivo nuevo o refactorizado en `src/` debe superar **400 líneas**. Extraer primitivos en `src/components/ui/` y subcomponentes por dominio.
- **Rendimiento 60 FPS:** Animar únicamente propiedades compuestas (`transform`, `opacity`). Respetar siempre `prefers-reduced-motion: reduce`.
- **Gestor de paquetes:** Usar `pnpm` para dependencias y scripts de frontend (`pnpm typecheck`, `pnpm lint`, `pnpm test`, `pnpm build`).
- **Build portable:** Al finalizar cualquier cambio verificado en `src/` o `src-tauri/`, actualizar `release-portable\ScanCleaner.exe` según `.agents/rules/release-portable.md`.
