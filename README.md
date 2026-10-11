# AppWindowsTreeSize

App de escritorio para Windows que analiza tus discos, te muestra qué ocupa espacio y te ayuda a liberarlo de forma segura.

## Estado

En construcción. Ya existe:

- **Motor de escaneo** (`crates/scanner`, Rust): recorre la unidad en paralelo, suma el tamaño de cada carpeta, clasifica los archivos por tipo (videos, imágenes, programas, instaladores, sistema, documentos, código, caché) y permite buscar, listar los archivos más grandes y descontar lo que se borra.
- **App Tauri** (`src-tauri`): lista los discos, lanza el análisis con progreso en vivo, abre el Explorador de Windows en una carpeta o archivo, envía elementos a la papelera y propone limpiezas (temporales, Windows.old, cachés de navegadores, npm y sombreadores, ISOs e instaladores en Descargas, `node_modules`, hibernación, papelera).

Falta la interfaz en React, que seguirá el [prototipo aprobado](https://claude.ai/artifact/S64GhgUozQuEzyvV7zD3zM).

## Requisitos para compilar en Windows

1. [Rust](https://rustup.rs) (estable).
2. [Node.js](https://nodejs.org) 22 o superior.
3. Microsoft C++ Build Tools y WebView2 (Windows 11 ya lo trae). Ver la [guía de Tauri](https://v2.tauri.app/start/prerequisites/).

## Pruebas del motor

```sh
cargo test -p scanner
```
