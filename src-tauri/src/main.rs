// Evita que se abra una consola junto a la ventana en las versiones de lanzamiento en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run()
}
