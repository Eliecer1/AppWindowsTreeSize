# AppWindowsTreeSize

App de escritorio para Windows que analiza tus discos, te muestra qué ocupa espacio y te ayuda a liberarlo de forma segura.

## Qué hace

- **Mientras analiza** ya puedes entrar en Explorar: las carpetas y archivos aparecen en cuanto se leen y los tamaños crecen en vivo.
- **Inicio**: uso de cada disco, cuánto puedes liberar, qué tipos de archivo ocupan más y los archivos más pesados.
- **Explorar**: mapa de bloques de cada carpeta. Haz clic en un bloque para entrar. El botón **Refrescar carpeta** vuelve a leer solo la carpeta que estás viendo y actualiza los totales de las de arriba. Desde cada elemento puedes abrir el Explorador de Windows o enviarlo a la papelera.
- **Limpiar**: sugerencias clasificadas por riesgo (temporales, Windows.old, Windows Update, cachés de navegadores, npm, pip y sombreadores, ISOs e instaladores en Descargas, `node_modules`, hibernación, papelera). Todo va a la papelera de reciclaje, así que se puede recuperar.
- **Tipos de archivo**: cuánto ocupa cada tipo en el disco.
- **Nube**: mide lo que cada archivo ocupa de verdad en el disco. Los archivos de OneDrive que están solo en la nube se marcan con una nube y no cuentan como espacio usado.
- **Ctrl+K**: busca por nombre, o por extensión escribiendo un punto (por ejemplo `.iso`).

## Cómo está hecho

| Parte | Carpeta | Tecnología |
|---|---|---|
| Motor de escaneo | `crates/scanner` | Rust, recorrido en paralelo con rayon |
| App de escritorio | `src-tauri` | Tauri 2 |
| Interfaz | `src` | React 19 + TypeScript + Vite |

La interfaz nunca recibe el árbol completo: pide a Rust solo la carpeta que está mostrando, así funciona igual con millones de archivos.

## Descargar el instalador

Cada cambio en `main` se compila en GitHub Actions. Entra en la pestaña **Actions**, abre la última ejecución de **Compilar** y descarga **AppWindowsTreeSize-instaladores** (trae el `.msi` y el `.exe`).

## Compilar en tu PC (Windows)

Requisitos: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 22 o superior y las [dependencias de Tauri para Windows](https://v2.tauri.app/start/prerequisites/) (Microsoft C++ Build Tools; WebView2 ya viene en Windows 11).

```sh
npm install
npm run tauri dev     # abre la app en modo desarrollo
npm run tauri build   # genera el instalador en target/release/bundle
```

Para analizar carpetas protegidas del sistema, ejecuta la app como administrador.

## Ver la interfaz en el navegador

```sh
npm install
npm run dev
```

Fuera de la app de escritorio la interfaz usa datos de ejemplo (se indica con la etiqueta "Datos de ejemplo").

## Pruebas

```sh
cargo test -p scanner
```
