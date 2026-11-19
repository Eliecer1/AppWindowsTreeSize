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
    /// Acción especial que hace la app en lugar de enviar a la papelera (por ejemplo, vaciarla).
    pub action: Option<String>,
}

struct Builder<'a> {
    tree: &'a Tree,
    out: Vec<Suggestion>,
}

impl Builder<'_> {
    fn size(&self, ids: &[NodeId]) -> u64 {
        ids.iter().map(|i| self.tree.size(*i)).sum()
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
                action: None,
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
                action: None,
            });
        }
    }

    fn add_action(&mut self, key: &str, title: &str, description: &str, risk: Risk, id: Option<NodeId>, action: &str) {
        let size = id.map(|i| self.size(&[i])).unwrap_or(0);
        if size > 0 {
            self.out.push(Suggestion {
                key: key.into(),
                title: title.into(),
                description: description.into(),
                risk,
                size,
                ids: Vec::new(),
                manual_hint: None,
                action: Some(action.into()),
            });
        }
    }

    /// Hijos directos de las carpetas dadas (perfiles de navegador, perfiles de Firefox…).
    fn subdirs(&self, parents: &[NodeId]) -> Vec<NodeId> {
        self.children_where(parents, |_, d| d)
    }

    fn find(&self, rel: &str) -> Option<NodeId> {
        self.tree.find(Path::new(rel))
    }

    /// Busca `rel` dentro de cada perfil de usuario (Users\<nombre>\...).
    fn in_profiles(&self, rel: &str) -> Vec<NodeId> {
        let Some(users) = self.find("Users") else { return Vec::new() };
        self.tree
            .kids(users)
            .iter()
            .filter(|p| self.tree.is_dir(**p))
            .filter_map(|p| self.find(&format!("Users/{}/{rel}", self.tree.name(*p))))
            .collect()
    }

    /// Hijos directos de las carpetas dadas cuyo nombre cumple `pred`.
    fn children_where(&self, parents: &[NodeId], pred: impl Fn(&str, bool) -> bool) -> Vec<NodeId> {
        parents
            .iter()
            .flat_map(|p| self.tree.kids(*p).iter().copied())
            .filter(|c| pred(self.tree.name(*c), self.tree.is_dir(*c)))
            .collect()
    }
}

/// Carpetas `node_modules` de primer nivel (no las anidadas dentro de otra).
fn node_modules(tree: &Tree) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![Tree::ROOT];
    while let Some(id) = stack.pop() {
        for &c in tree.kids(id) {
            if !tree.is_dir(c) {
                continue;
            }
            let name = tree.name(c);
            if name == "node_modules" {
                found.push(c);
            } else if !matches!(name, "Windows" | "$Recycle.Bin" | "System Volume Information") {
                stack.push(c);
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
    // Firefox: Profiles\<perfil>\cache2 en Local.
    let ff = b.in_profiles("AppData/Local/Mozilla/Firefox/Profiles");
    let ff_profiles = b.subdirs(&ff);
    browser.extend(b.children_where(&ff_profiles, |n, d| d && n.eq_ignore_ascii_case("cache2")));
    b.add("browser-cache", "Caché de navegadores", "Copias de páginas web de Chrome, Edge, Brave y Firefox. Se vuelven a descargar cuando hagan falta.", Risk::Safe, browser);

    let mut apps = Vec::new();
    for base in ["AppData/Roaming/discord", "AppData/Roaming/Microsoft/Teams", "AppData/Roaming/Slack", "AppData/Roaming/Code"] {
        let dirs = b.in_profiles(base);
        apps.extend(b.children_where(&dirs, |n, d| d && matches!(n, "Cache" | "Code Cache" | "GPUCache" | "CachedData" | "Service Worker")));
    }
    apps.extend(b.in_profiles("AppData/Local/Packages/MSTeams_8wekyb3d8bbwe/LocalCache"));
    apps.extend(b.in_profiles("AppData/Local/Spotify/Data"));
    b.add("app-cache", "Caché de Discord, Teams, Slack, Spotify y VS Code", "Archivos temporales de estas apps. Se regeneran solos; puede que tarden un poco más en abrir la primera vez.", Risk::Safe, apps);

    let mut dumps = b.in_profiles("AppData/Local/CrashDumps");
    dumps.extend(b.find("Windows/Minidump"));
    dumps.extend(b.find("Windows/LiveKernelReports"));
    dumps.extend(b.find("Windows/MEMORY.DMP"));
    dumps.extend(b.find("ProgramData/Microsoft/Windows/WER/ReportArchive"));
    dumps.extend(b.find("ProgramData/Microsoft/Windows/WER/ReportQueue"));
    b.add("crash-dumps", "Volcados de errores", "Informes que Windows guarda cuando un programa o el sistema se bloquea. Solo sirven para diagnosticar fallos.", Risk::Safe, dumps);

    let delivery: Vec<_> = b
        .find("Windows/ServiceProfiles/NetworkService/AppData/Local/Microsoft/Windows/DeliveryOptimization/Cache")
        .or_else(|| b.find("Windows/SoftwareDistribution/DeliveryOptimization"))
        .into_iter()
        .collect();
    b.add("delivery-optimization", "Optimización de distribución", "Copias de actualizaciones que Windows comparte con otros equipos. Se pueden borrar sin problema.", Risk::Safe, delivery);

    let mut dev = b.in_profiles("AppData/Local/npm-cache");
    dev.extend(b.in_profiles("AppData/Local/pip/cache"));
    dev.extend(b.in_profiles("AppData/Local/Yarn/Cache"));
    b.add("dev-cache", "Caché de npm, pip y Yarn", "Paquetes descargados que se pueden volver a bajar.", Risk::Safe, dev);

    let mut pkgs = b.in_profiles(".nuget/packages");
    pkgs.extend(b.in_profiles(".gradle/caches"));
    pkgs.extend(b.in_profiles(".cargo/registry"));
    pkgs.extend(b.in_profiles(".m2/repository"));
    b.add("package-cache", "Paquetes de NuGet, Gradle, Cargo y Maven", "Dependencias descargadas de proyectos de programación. Se vuelven a bajar al compilar, lo que puede tardar.", Risk::Review, pkgs);

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
    b.add_action(
        "recycle-bin",
        "Vaciar la papelera de reciclaje",
        "Archivos que ya borraste y siguen ocupando espacio. Al vaciarla ya no se pueden recuperar.",
        Risk::Review,
        bin,
        "empty-recycle-bin",
    );

    b.out.sort_by(|a, b| (a.risk as u8).cmp(&(b.risk as u8)).then(b.size.cmp(&a.size)));
    b.out
}
