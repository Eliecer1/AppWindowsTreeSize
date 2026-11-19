# AppWindowsTreeSize

App de escritorio para Windows que analiza tus discos, te muestra qué ocupa espacio y te ayuda a liberarlo de forma segura.

## Qué hace

- **Rápido como administrador**: en discos NTFS lee directamente la tabla de archivos (MFT) y analiza un disco entero en segundos. Sin permisos de administrador recorre las carpetas en paralelo; la app ofrece **Reiniciar como administrador**.
- **Mientras analiza** ya puedes entrar en Explorar: las carpetas y archivos aparecen en cuanto se leen y los tamaños crecen en vivo. La ventana no se congela en ningún momento.
- **Tamaños correctos**: los enlaces duros (WinSxS, por ejemplo) se cuentan una sola vez, y los archivos de OneDrive que están solo en la nube no cuentan como espacio usado.
- **Inicio**: uso de cada disco, cuánto puedes liberar, qué tipos de archivo ocupan más y los archivos más pesados. **Analizar una carpeta…** analiza solo la carpeta que elijas.
- **Explorar**: mapa de bloques con dos niveles, o vista de **Anillos**. Clic derecho para abrir en el Explorador, copiar la ruta, refrescar, enviar a la papelera o borrar definitivamente. Ctrl y Mayús seleccionan varios. Se maneja con el teclado (flechas, Intro, Retroceso, Alt+←/→, Supr, F5). **Exportar** guarda un informe en HTML o CSV.
- **Archivos**: los archivos más grandes con filtros por tamaño, antigüedad ("sin cambios en 1 año"), tipo y extensión.
- **Duplicados**: encuentra archivos con el mismo contenido (compara el contenido, no el nombre) y deja solo la copia más reciente.
- **Limpiar**: sugerencias clasificadas por riesgo (temporales, Windows.old, Windows Update, Optimización de distribución, volcados de errores, cachés de navegadores y de Discord, Teams, Slack, Spotify y VS Code, npm, pip, NuGet, Gradle, Cargo y Maven, ISOs e instaladores en Descargas, `node_modules`, hibernación y vaciar la papelera).
- **Cambios**: cada análisis se guarda. Al volver a analizar ves qué carpetas crecieron, qué es nuevo y qué se borró. Al abrir la app, los últimos análisis ya están cargados.
- **Tipos de archivo**: cuánto ocupa cada tipo en el disco.
- **Ctrl+K**: busca por nombre, o por extensión escribiendo un punto (por ejemplo `.iso`).
- **Menú del Explorador**: clic derecho en una carpeta o disco → "Analizar con AppWindowsTreeSize".
- **Español e inglés**: el botón ES/EN de la barra superior cambia el idioma.
- **Actualizaciones automáticas** desde GitHub Releases (ver abajo).

## Cómo está hecho

| Parte | Carpeta | Tecnología |
|---|---|---|
| Motor de escaneo | `crates/scanner` | Rust: lector de MFT, recorrido en paralelo con rayon, árbol compacto (40 bytes por elemento) |
| App de escritorio | `src-tauri` | Tauri 2 |
| Interfaz | `src` | React 19 + TypeScript + Vite |

La interfaz nunca recibe el árbol completo: pide a Rust solo la carpeta que está mostrando, así funciona igual con millones de archivos.

## Descargar el instalador

Cada cambio en `main` se compila en GitHub Actions. Entra en la pestaña **Actions**, abre la última ejecución de **Compilar** y descarga **AppWindowsTreeSize-instaladores** (trae el `.msi` y el `.exe`).

## Publicar una versión con actualizaciones automáticas

1. Genera la clave de firma de actualizaciones una sola vez: `npx tauri signer generate -w ~/.tauri/awts.key`. Guarda la clave privada fuera del repositorio.
2. Copia la clave pública en `plugins.updater.pubkey` de `src-tauri/tauri.conf.json`.
3. En GitHub, en **Settings → Secrets and variables → Actions**, crea `TAURI_SIGNING_PRIVATE_KEY` (el contenido de la clave privada) y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Sube la versión en `tauri.conf.json` y crea una etiqueta `v0.2.0`. El flujo **Publicar** compila, firma y publica la versión; las apps instaladas la detectan solas.

Para que Windows no muestre el aviso de SmartScreen hace falta además un certificado de firma de código. Si lo tienes, añade `WINDOWS_CERTIFICATE` (el .pfx en base64), `WINDOWS_CERTIFICATE_PASSWORD`; el flujo lo detecta solo.

## Compilar en tu PC (Windows)

Requisitos: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 22 o superior y las [dependencias de Tauri para Windows](https://v2.tauri.app/start/prerequisites/) (Microsoft C++ Build Tools; WebView2 ya viene en Windows 11).

```sh
npm install
npm run tauri dev     # abre la app en modo desarrollo
npm run tauri build   # genera el instalador en target/release/bundle
```

Como administrador el análisis usa la MFT y es mucho más rápido, y puede leer carpetas protegidas del sistema.

## Ver la interfaz en el navegador

```sh
npm install
npm run dev
```

Fuera de la app de escritorio la interfaz usa datos de ejemplo (se indica con la etiqueta "Datos de ejemplo").

## Pruebas

```sh
cargo test -p scanner   # incluye una imagen NTFS de prueba para el lector de MFT
```
