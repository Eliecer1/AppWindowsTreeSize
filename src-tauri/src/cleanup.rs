//! Sugerencias de limpieza: lugares conocidos de Windows que suelen acumular espacio recuperable.

use scanner::{extension, NodeId, Tree};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Review,
    Careful,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub key: String,
    pub title: String,
    pub description: String,
    pub risk: Risk,
    pub size: u64,
    /// Elementos que se envían a la papelera. Vacío cuando la limpieza se hace desde Windows.
    pub ids: Vec<NodeId>,
    /// Qué hacer cuando no se puede borrar directamente.
    pub manual_hint: Option<String>,
}

struct Builder<'a> {
    tree: &'a Tree,
    out: Vec<Suggestion>,
}

impl Builder<'_> {
    fn size(&self, ids: &[NodeId]) -> u64 {
        ids.iter().filter_map(|i| self.tree.entry(*i)).map(|e| e.size).sum()
    }

    fn add(&mut self, key: &str, title: &str, description: &str, risk: Risk, ids: Vec<NodeId>) {
        let size = self.size(&ids);
        if size > 0 {
            self.out.push(Suggestion {
                key: key.into(),
                title: title.into(),
                description: description.into(),
                risk,
                size,
                ids,
                manual_hint: None,
            });
        }
    }

    fn add_manual(&mut self, key: &str, title: &str, description: &str, risk: Risk, id: Option<NodeId>, hint: &str) {
        let size = id.map(|i| self.size(&[i])).unwrap_or(0);
        if size > 0 {
            self.out.push(Suggestion {
                key: key.into(),
                title: title.into(),
                description: description.into(),
                risk,
                size,
                ids: Vec::new(),
                manual_hint: Some(hint.into()),
            });
        }
    }

    fn find(&self, rel: &str) -> Option<NodeId> {
        self.tree.find(Path::new(rel))
    }

    /// Busca `rel` dentro de cada perfil de usuario (Users\<nombre>\...).
    fn in_profiles(&self, rel: &str) -> Vec<NodeId> {
        let Some(users) = self.find("Users") else { return Vec::new() };
        let profiles = self.tree.entry(users).map(|e| e.children.clone()).unwrap_or_default();
        profiles
            .into_iter()
            .filter_map(|p| {
                let name = &self.tree.entry(p)?.name;
                self.find(&format!("Users/{name}/{rel}"))
            })
            .collect()
    }

    /// Hijos directos de las carpetas dadas cuyo nombre cumple `pred`.
    fn children_where(&self, parents: &[NodeId], pred: impl Fn(&str, bool) -> bool) -> Vec<NodeId> {
        parents
            .iter()
            .filter_map(|p| self.tree.entry(*p))
            .flat_map(|e| e.children.iter().copied())
            .filter(|c| self.tree.entry(*c).map(|e| pred(&e.name, e.is_dir)).unwrap_or(false))
            .collect()
    }
}

/// Carpetas `node_modules` de primer nivel (no las anidadas dentro de otra).
fn node_modules(tree: &Tree) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![Tree::ROOT];
    while let Some(id) = stack.pop() {
        let Some(e) = tree.entry(id) else { continue };
        for c in &e.children {
            let ce = tree.entry(*c).unwrap();
            if !ce.is_dir {
                continue;
            }
            if &*ce.name == "node_modules" {
                found.push(*c);
            } else if !matches!(&*ce.name, "Windows" | "$Recycle.Bin" | "System Volume Information") {
                stack.push(*c);
            }
        }
    }
    found
}

pub fn suggestions(tree: &Tree) -> Vec<Suggestion> {
    let mut b = Builder { tree, out: Vec::new() };

    let mut temps = b.in_profiles("AppData/Local/Temp");
    temps.extend(b.find("Windows/Temp"));
    b.add("temp", "Archivos temporales", "Archivos que Windows y los programas dejan en Temp y ya no usan.", Risk::Safe, temps);

    let old: Vec<_> = b.find("Windows.old").into_iter().collect();
    b.add(
        "windows-old",
        "Instalación anterior de Windows",
        "La carpeta Windows.old que quedó tras una actualización. Solo sirve para volver a la versión anterior.",
        Risk::Safe,
        old,
    );

    let wu: Vec<_> = b.find("Windows/SoftwareDistribution/Download").into_iter().collect();
    b.add("windows-update", "Descargas de Windows Update", "Archivos de actualizaciones que ya se instalaron.", Risk::Safe, wu);

    let mut browser = Vec::new();
    for base in ["AppData/Local/Google/Chrome/User Data", "AppData/Local/Microsoft/Edge/User Data", "AppData/Local/BraveSoftware/Brave-Browser/User Data"] {
        let profiles = b.in_profiles(base);
        let profile_dirs = b.children_where(&profiles, |_, d| d);
        browser.extend(b.children_where(&profile_dirs, |n, d| d && matches!(n, "Cache" | "Code Cache" | "GPUCache" | "Service Worker")));
    }
    b.add("browser-cache", "Caché de navegadores", "Copias de páginas web de Chrome, Edge y Brave. Se vuelven a descargar cuando hagan falta.", Risk::Safe, browser);

    let mut dev = b.in_profiles("AppData/Local/npm-cache");
    dev.extend(b.in_profiles("AppData/Local/pip/cache"));
    dev.extend(b.in_profiles("AppData/Local/Yarn/Cache"));
    b.add("dev-cache", "Caché de npm, pip y Yarn", "Paquetes descargados que se pueden volver a bajar.", Risk::Safe, dev);

    let mut shaders = b.in_profiles("AppData/Local/NVIDIA/DXCache");
    shaders.extend(b.in_profiles("AppData/Local/NVIDIA/GLCache"));
    shaders.extend(b.in_profiles("AppData/Local/AMD/DxCache"));
    shaders.extend(b.in_profiles("AppData/Local/D3DSCache"));
    b.add("shader-cache", "Caché de sombreadores", "Los juegos la regeneran la próxima vez que se abren.", Risk::Safe, shaders);

    let downloads = b.in_profiles("Downloads");
    let isos = b.children_where(&downloads, |n, d| !d && matches!(extension(n).as_str(), "iso" | "img"));
    b.add("iso", "Imágenes ISO en Descargas", "Imágenes de discos de instalación. Revisa si todavía las necesitas.", Risk::Review, isos);
    let installers = b.children_where(&downloads, |n, d| !d && matches!(extension(n).as_str(), "exe" | "msi"));
    b.add("installers", "Instaladores en Descargas", "Archivos .exe y .msi que normalmente ya no hacen falta después de instalar.", Risk::Review, installers);

    b.add("node-modules", "Carpetas node_modules", "Dependencias de proyectos. Se recuperan con npm install.", Risk::Review, node_modules(tree));

    let hib = b.find("hiberfil.sys");
    b.add_manual(
        "hibernation",
        "Archivo de hibernación",
        "Desactivar la hibernación libera hiberfil.sys, pero quita la opción Hibernar del menú de apagado.",
        Risk::Careful,
        hib,
        "Abre una terminal como administrador y ejecuta: powercfg /hibernate off",
    );

    let bin = b.find("$Recycle.Bin");
    b.add_manual(
        "recycle-bin",
        "Papelera de reciclaje",
        "Archivos que ya borraste y siguen ocupando espacio.",
        Risk::Safe,
        bin,
        "Haz clic derecho en la Papelera del escritorio y elige Vaciar papelera de reciclaje.",
    );

    b.out.sort_by(|a, b| (a.risk as u8).cmp(&(b.risk as u8)).then(b.size.cmp(&a.size)));
    b.out
}
