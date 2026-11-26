# DESIGN.md — ScanCleaner (v2 — Bento & Neumorphic Launcher)

> **Modo de superficie:** `Operate` con bienvenida tipo **App Launcher + Bento Dashboard** (inspirado en la claridad luminosa de Proton y el craft de herramientas nativas modernas).
> **Tema:** **Dual autodetectado por el sistema** (`prefers-color-scheme`) con toggle manual persistido (`Claro` luminoso por defecto cuando el sistema es claro, `Oscuro` obsidiana neumórfico cuando es oscuro).

---

## 1. Dirección visual y principios

- **Aprovechamiento máximo del espacio:** Sin sidebar fijo de 224px. Barra superior limpia y translúcida (`56px`) que deja el **100% del ancho** disponible para el Bento Grid de Inicio, el Treemap y las tablas virtualizadas.
- **Inicio unificado (Launcher + Bento de Unidades):**
  - Arriba: **Launcher de accesos directos** con 6 tiles *squircle* neumórficos (`rounded-[22px]`) e íconos SVG propios con degradados vibrantes por módulo, elevación táctil y ligero tilt/lift en hover.
  - Debajo: **Bento Grid de Unidades** con tarjeta de resumen global (anillo SVG de almacenamiento total usado/libre) + tarjetas Bento por unidad listas para escanear en 1 clic.
- **Superficies neumórficas suaves:** Elevación mediante doble luz (borde interno superior sutil + sombra ambiental multicapa suave) combinada con bordes finos de `1px` para legibilidad nítida en Windows WebView2.
- **Gradient Mesh + Parallax 60 FPS:** Fondo atmosférico con orbes radiales suaves que responden al puntero mediante `transform: translate3d(...)` en GPU (sin re-renders de React, desactivado automáticamente en `prefers-reduced-motion: reduce`).

---

## 2. Tokens semánticos Duales (Claro / Oscuro)

| Token CSS | Claro (`[data-theme="light"]`) | Oscuro (`[data-theme="dark"]`) | Rol |
|---|---|---|---|
| `--surface-canvas` | `#f5f4fb` | `#0b0d14` | Fondo raíz de ventana y cavidades |
| `--surface-shell` | `rgba(255, 255, 255, 0.78)` | `rgba(14, 17, 26, 0.82)` | Topbar translúcida y encabezados de tabla |
| `--surface-panel` | `#ffffff` | `#131724` | Tarjetas Bento, paneles y modales |
| `--surface-elevated` | `#f8f8fd` | `#1a2032` | Tiles elevados, hover de filas, controles |
| `--surface-active` | `#efeff9` | `#232a40` | Estado activo/presionado de controles |
| `--text-primary` | `#161624` | `#f1f3fa` | Títulos, valores principales, nombres |
| `--text-secondary` | `#555870` | `#9ca4bc` | Subtítulos, metadatos, descripciones |
| `--text-muted` | `#8488a0` | `#6c738c` | Placeholders, fechas secundarias |
| `--accent` | `#4f46e5` / `#2563eb` (`#3b82f6`) | `#5caeff` | Acento principal de foco y navegación |
| `--accent-strong` | `#2563eb` | `#2574d9` | CTA primario |
| `--success` | `#16a34a` | `#2eb872` | Espacio libre / nivel seguro |
| `--warning` | `#d97706` | `#e09f3e` | Elevación UAC / revisar / protegidas |
| `--danger` | `#dc2626` | `#e05555` | Acciones destructivas / discos >90% |

---

## 3. Tipografía y movimiento

- **Fuentes offline:** `@fontsource/geist-sans` + `@fontsource/jetbrains-mono` con `tabular-nums` en todas las métricas de disco.
- **Movimiento (`emil-design-eng`):**
  - `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`
  - Hover en tiles squircle: `translateY(-4px) scale(1.02)` en `200ms var(--ease-out)` con transición de sombra neumórfica.
  - Presión táctil: `scale(0.97)` en `120ms`.
  - Entrada coreografiada en Inicio (`stagger` sutil de `35ms` por bloque, `opacity` + `translateY(8px)`).
  - `prefers-reduced-motion: reduce`: desactiva parallax, animaciones de entrada y transiciones no esenciales.
