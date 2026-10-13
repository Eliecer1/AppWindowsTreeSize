// Datos de ejemplo para ver la interfaz en un navegador, sin el motor de Rust.
// Imita el comportamiento de crates/scanner: ids, hijos ordenados por tamaño y totales por carpeta.

import type { Api } from "./api";
import type { Category, Drive, NodeInfo, ScanDone, ScanProgress, Suggestion } from "./types";

const GB = 1024 ** 3;
const MB = 1024 ** 2;

let seed = 20251011;
function rnd() {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const hex = (n: number) => Array.from({ length: n }, () => "0123456789abcdef"[Math.floor(rnd() * 16)]).join("");
const pad = (n: number, l = 2) => String(n).padStart(l, "0");
const T0 = Date.UTC(2023, 0, 1) / 1000;
const T1 = Date.UTC(2025, 9, 10) / 1000;

interface Spec {
  n: string;
  s?: number;
  c?: Category;
  ch?: Spec[];
}
const F = (n: string, s: number, c?: Category): Spec => ({ n, s, c });
const D = (n: string, ...ch: (Spec | Spec[])[]): Spec => ({ n, ch: ch.flat() });
function gen(count: number, total: number, name: (i: number) => string, c?: Category): Spec[] {
  const w = Array.from({ length: count }, () => rnd() ** 2 + 0.08);
  const sum = w.reduce((a, b) => a + b, 0);
  return w.map((x, i) => F(name(i), (total * x) / sum, c));
}

const EXT: Record<string, Category> = {
  mp4: "video", mkv: "video", mov: "video", jpg: "image", png: "image", heic: "image", nef: "image", cr3: "image",
  exe: "app", dll: "app", pak: "app", archive: "app", vpk: "app", iso: "archive", zip: "archive", msi: "archive",
  msp: "archive", cab: "archive", sys: "system", vhdx: "system", etl: "system", pdf: "docs", docx: "docs",
  xlsx: "docs", pptx: "docs", txt: "docs", js: "code", json: "code", ts: "code", tsx: "code", tmp: "cache",
  log: "cache", nvph: "cache",
};
const extOf = (n: string) => {
  const i = n.lastIndexOf(".");
  return i > 0 && n.length - i <= 10 ? n.slice(i + 1).toLowerCase() : "";
};

function cDrive(): Spec {
  return D("C:\\",
    D("Users", D("Eliecer",
      D("OneDrive",
        D("Fotos", gen(900, 96 * GB, (i) => `IMG_${pad(1000 + i, 4)}.jpg`)),
        D("Videos familiares", gen(70, 84 * GB, (i) => `Video ${pad(i + 1, 3)}.mp4`)),
        D("Documentos", gen(300, 6 * GB, (i) => [`Informe ${i}.pdf`, `Hoja ${i}.xlsx`, `Carta ${i}.docx`][i % 3]))),
      D("Videos",
        D("Grabaciones OBS", gen(42, 141 * GB, (i) => `2025-${pad(1 + (i % 9))}-${pad(1 + (i % 27))} 21-${pad(i)}-03.mkv`)),
        D("Vacaciones 2025", gen(60, 58 * GB, (i) => `VID_202507${pad(1 + Math.floor(i / 4))}_${hex(6)}.mp4`)),
        D("Clips de juegos", gen(80, 23 * GB, (i) => `Clip ${pad(i + 1, 3)}.mp4`))),
      D("Downloads",
        F("Win11_24H2_Spanish_x64.iso", 5.8 * GB), F("ubuntu-24.04.1-desktop-amd64.iso", 6.1 * GB),
        F("Windows Server 2022.iso", 4.7 * GB), F("UnrealEngine-5.5-Offline.exe", 13.2 * GB),
        F("Adobe_Premiere_Pro_2025_Setup.exe", 3.9 * GB), F("Docker Desktop Installer.exe", 612 * MB),
        F("node-v22.11.0-x64.msi", 29 * MB), F("Proyecto final (respaldo).zip", 4.2 * GB),
        gen(30, 1.4 * GB, (i) => [`Factura ${pad(i, 3)}.pdf`, `documento (${i}).docx`, `foto-${hex(4)}.jpg`][i % 3])),
      D("AppData",
        D("Local",
          D("Temp", gen(300, 14 * GB, () => `~DF${hex(4).toUpperCase()}.tmp`, "cache")),
          D("Google", D("Chrome", D("User Data", D("Default", D("Cache", gen(200, 9 * GB, () => `f_0${hex(5)}`, "cache")))))),
          D("npm-cache", gen(200, 7 * GB, () => hex(40), "cache")),
          D("NVIDIA", D("DXCache", gen(120, 6 * GB, () => `${hex(16)}.nvph`, "cache"))),
          D("Packages", gen(40, 12 * GB, (i) => `Microsoft.App${i}_${hex(8)}.dat`, "app"))),
        D("Roaming", gen(60, 9 * GB, () => `${hex(10)}.dat`, "app"))),
      D("Pictures",
        D("Fotos del teléfono", gen(400, 38 * GB, (i) => `IMG_${pad(4000 + i, 4)}.heic`)),
        D("Screenshots", gen(200, 4 * GB, (i) => `Captura de pantalla ${pad(i, 3)}.png`))),
      D("Documents", gen(160, 18 * GB, (i) => [`Contrato ${i}.pdf`, `Presupuesto ${i}.xlsx`, `Propuesta ${i}.docx`, `Demo ${i}.mp4`][i % 4])),
      D("Projects",
        D("tienda-online", D("node_modules", gen(120, 3.9 * GB, (i) => `pkg${i}.js`, "code")), D("src", gen(60, 40 * MB, (i) => `App${i}.tsx`))),
        D("api-pagos", D("node_modules", gen(90, 2.6 * GB, (i) => `pkg${i}.js`, "code")))))),
    D("Program Files (x86)", D("Steam", D("steamapps", D("common",
      D("Baldur's Gate 3", F("Gustav.pak", 46.2 * GB), F("GustavX.pak", 21.4 * GB), gen(18, 52 * GB, (i) => `VirtualTextures_${i}.pak`)),
      D("Cyberpunk 2077", gen(26, 71 * GB, (i) => `basegame_${i}.archive`)),
      D("Counter-Strike 2", gen(120, 34 * GB, (i) => `pak01_${pad(i, 3)}.vpk`)))))),
    D("Program Files",
      D("Adobe", gen(200, 21 * GB, () => `${hex(8)}.dll`)),
      D("Microsoft Office", gen(120, 6 * GB, () => `${hex(8)}.dll`)),
      D("Microsoft Visual Studio", gen(300, 14 * GB, () => `${hex(10)}.dll`))),
    D("Windows",
      D("WinSxS", gen(200, 12 * GB, () => `${hex(8)}.dll`)),
      D("Installer", gen(80, 11 * GB, () => `${hex(7)}.msi`)),
      D("System32", gen(200, 8 * GB, () => `${hex(6)}.dll`)),
      D("SoftwareDistribution", D("Download", gen(10, 6.2 * GB, () => `${hex(40)}.cab`)))),
    D("Windows.old", gen(100, 22 * GB, () => `${hex(8)}.dll`)),
    F("pagefile.sys", 16 * GB), F("hiberfil.sys", 12.8 * GB));
}

function dDrive(): Spec {
  return D("D:\\",
    D("Respaldos", gen(8, 418 * GB, (i) => `backup_part${i + 1}.zip`)),
    D("Películas", gen(48, 568 * GB, (i) => `Película ${i + 1} 2160p.mkv`)),
    D("Fotos RAW", gen(600, 263 * GB, (i) => `_DSC${pad(i, 4)}.CR3`)),
    D("Máquinas virtuales", F("Ubuntu 24.04.vhdx", 64 * GB), F("Windows Dev.vhdx", 80 * GB)));
}

interface Entry {
  name: string;
  parent: number;
  isDir: boolean;
  size: number;
  logical: number;
  cloud: number;
  cloudFiles: number;
  files: number;
  dirs: number;
  modified: number;
  category: Category;
  byCat: Partial<Record<Category, number>>;
  children: number[];
  gone?: boolean;
}

class DemoTree {
  entries: Entry[] = [];
  constructor(spec: Spec) {
    this.add(spec, -1, false, false);
  }
  private add(s: Spec, parent: number, cache: boolean, inCloud: boolean): number {
    const id = this.entries.length;
    const isCache = cache || ["Temp", "Cache", "npm-cache", "DXCache"].includes(s.n);
    const isCloud = inCloud || s.n === "OneDrive";
    const e: Entry = { name: s.n, parent, isDir: !!s.ch, size: 0, logical: 0, cloud: 0, cloudFiles: 0, files: 0, dirs: 0, modified: 0, category: "other", byCat: {}, children: [] };
    this.entries.push(e);
    if (!s.ch) {
      e.logical = Math.round(s.s ?? 0);
      // En OneDrive, la mayoría de los archivos están "solo en línea": no ocupan espacio local.
      const cloudOnly = isCloud && rnd() < 0.8;
      e.size = cloudOnly ? 0 : e.logical;
      e.cloud = cloudOnly ? e.logical : 0;
      e.cloudFiles = cloudOnly ? 1 : 0;
      e.files = 1;
      e.modified = Math.round(T0 + rnd() * (T1 - T0));
      e.category = s.c ?? (isCache ? "cache" : EXT[extOf(s.n)] ?? "other");
      e.byCat = { [e.category]: e.size };
      return id;
    }
    const kids = s.ch.map((c) => this.add(c, id, isCache, isCloud));
    kids.sort((a, b) => this.entries[b].size - this.entries[a].size);
    e.children = kids;
    for (const k of kids) {
      const c = this.entries[k];
      e.size += c.size;
      e.logical += c.logical;
      e.cloud += c.cloud;
      e.cloudFiles += c.cloudFiles;
      e.files += c.files;
      e.dirs += c.isDir ? 1 + c.dirs : 0;
      e.modified = Math.max(e.modified, c.modified);
      for (const [cat, v] of Object.entries(c.byCat)) e.byCat[cat as Category] = (e.byCat[cat as Category] ?? 0) + (v ?? 0);
    }
    e.category = (Object.entries(e.byCat).sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0))[0]?.[0] as Category) ?? "other";
    return id;
  }
  path(id: number): string {
    const parts: string[] = [];
    for (let cur = id; cur > 0; cur = this.entries[cur].parent) parts.unshift(this.entries[cur].name);
    return this.entries[0].name + parts.join("\\");
  }
  info(id: number): NodeInfo {
    const e = this.entries[id];
    return {
      id, parent: e.parent >= 0 ? e.parent : null, name: e.name, path: this.path(id), isDir: e.isDir, size: e.size, logical: e.logical, cloud: e.cloud, cloudFiles: e.cloudFiles,
      cloudOnly: !e.isDir && e.cloudFiles === 1, files: e.files, dirs: e.dirs,
      modified: e.modified, ext: e.isDir ? "" : extOf(e.name), category: e.category,
      hasChildren: e.children.some((c) => this.entries[c].isDir),
    };
  }
  find(rel: string): number | undefined {
    let cur = 0;
    for (const part of rel.split("/")) {
      const next = this.entries[cur].children.find((c) => this.entries[c].name.toLowerCase() === part.toLowerCase());
      if (next === undefined) return undefined;
      cur = next;
    }
    return cur;
  }
  remove(id: number): number {
    const e = this.entries[id];
    if (e.gone || id === 0) return 0;
    const p = this.entries[e.parent];
    p.children = p.children.filter((c) => c !== id);
    for (let cur = e.parent; cur >= 0; cur = this.entries[cur].parent) {
      const a = this.entries[cur];
      a.size -= e.size;
      a.logical -= e.logical;
      a.cloud -= e.cloud;
      a.cloudFiles -= e.cloudFiles;
      a.files -= e.files;
      a.dirs -= e.isDir ? 1 + e.dirs : 0;
      for (const [cat, v] of Object.entries(e.byCat)) a.byCat[cat as Category] = (a.byCat[cat as Category] ?? 0) - (v ?? 0);
    }
    e.gone = true;
    return e.size;
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function demoApi(): Api {
  const specs: Record<string, () => Spec> = { "C:\\": cDrive, "D:\\": dDrive };
  const caps: Record<string, number> = { "C:\\": 953.3 * GB, "D:\\": 1862.9 * GB };
  const trees: Record<string, DemoTree> = { "C:\\": new DemoTree(cDrive()) };
  const progressCbs: ((p: ScanProgress) => void)[] = [];
  const doneCbs: ((d: ScanDone) => void)[] = [];
  let cancelled = false;
  const tree = (root: string) => {
    const t = trees[root];
    if (!t) throw new Error(`Todavía no se ha analizado ${root}`);
    return t;
  };
  const live = (t: DemoTree) =>
    t.entries
      .map((_, i) => i)
      .filter((i) => {
        for (let cur = i; cur > 0; cur = t.entries[cur].parent) if (t.entries[cur].gone) return false;
        return i > 0;
      });

  return {
    isDemo: true,
    async listDrives(): Promise<Drive[]> {
      return Object.keys(specs).map((root) => {
        const used = trees[root]?.entries[0].size ?? (root === "C:\\" ? 0 : 1430 * GB);
        return {
          root, name: root === "C:\\" ? "Windows" : "Datos", total: caps[root], available: caps[root] - used,
          kind: root === "C:\\" ? "SSD" : "HDD", removable: false, fileSystem: "NTFS", scanned: !!trees[root],
        };
      });
    },
    async startScan(root) {
      cancelled = false;
      const target = new DemoTree(specs[root]());
      const totalFiles = target.entries[0].files * 410;
      const totalBytes = target.entries[0].size;
      const started = performance.now();
      (async () => {
        const dur = 2600;
        for (;;) {
          const t = Math.min((performance.now() - started) / dur, 1);
          const p = { root, files: Math.round(totalFiles * t), dirs: Math.round(target.entries[0].dirs * 90 * t), bytes: Math.round(totalBytes * t), elapsedMs: performance.now() - started };
          progressCbs.forEach((cb) => cb(p));
          if (t >= 1 || cancelled) {
            if (!cancelled) trees[root] = target;
            doneCbs.forEach((cb) => cb({ ...p, errors: 0, cancelled }));
            return;
          }
          await sleep(100);
        }
      })();
    },
    async cancelScan() {
      cancelled = true;
    },
    async onProgress(cb) {
      progressCbs.push(cb);
      return () => progressCbs.splice(progressCbs.indexOf(cb), 1);
    },
    async onDone(cb) {
      doneCbs.push(cb);
      return () => doneCbs.splice(doneCbs.indexOf(cb), 1);
    },
    async getFolder(root, id, limit) {
      const t = tree(root);
      const e = t.entries[id];
      const ancestors: NodeInfo[] = [];
      for (let cur = id; cur >= 0; cur = t.entries[cur].parent) ancestors.unshift(t.info(cur));
      return {
        node: t.info(id),
        ancestors,
        children: e.children.slice(0, limit).map((c) => t.info(c)),
        totalChildren: e.children.length,
        categories: Object.entries(e.byCat)
          .map(([category, size]) => ({ category: category as Category, size: size ?? 0 }))
          .filter((c) => c.size > 0)
          .sort((a, b) => b.size - a.size),
      };
    },
    async getChildren(root, id, offset, limit) {
      const t = tree(root);
      return t.entries[id].children.slice(offset, offset + limit).map((c) => t.info(c));
    },
    async largestFiles(root, id, limit) {
      const t = tree(root);
      const out: number[] = [];
      const stack = [id];
      while (stack.length) {
        const cur = stack.pop()!;
        const e = t.entries[cur];
        if (e.isDir) stack.push(...e.children);
        else out.push(cur);
      }
      return out.sort((a, b) => t.entries[b].size - t.entries[a].size).slice(0, limit).map((i) => t.info(i));
    },
    async search(root, query, limit) {
      const t = tree(root);
      const q = query.trim().toLowerCase();
      if (!q) return [];
      const ext = q.startsWith(".") ? q.slice(1) : null;
      return live(t)
        .filter((i) => {
          const e = t.entries[i];
          return ext !== null ? !e.isDir && extOf(e.name) === ext : e.name.toLowerCase().includes(q);
        })
        .sort((a, b) => t.entries[b].size - t.entries[a].size)
        .slice(0, limit)
        .map((i) => t.info(i));
    },
    async cleanupSuggestions(root) {
      const t = tree(root);
      const out: Suggestion[] = [];
      const add = (key: string, title: string, description: string, risk: Suggestion["risk"], ids: (number | undefined)[], manualHint: string | null = null) => {
        const valid = ids.filter((i): i is number => i !== undefined && !t.entries[i].gone);
        const size = valid.reduce((a, i) => a + t.entries[i].size, 0);
        if (size > 0) out.push({ key, title, description, risk, size, ids: manualHint ? [] : valid, manualHint });
      };
      add("temp", "Archivos temporales", "Archivos que Windows y los programas dejan en Temp y ya no usan.", "safe", [t.find("Users/Eliecer/AppData/Local/Temp")]);
      add("windows-old", "Instalación anterior de Windows", "La carpeta Windows.old que quedó tras una actualización. Solo sirve para volver a la versión anterior.", "safe", [t.find("Windows.old")]);
      add("windows-update", "Descargas de Windows Update", "Archivos de actualizaciones que ya se instalaron.", "safe", [t.find("Windows/SoftwareDistribution/Download")]);
      add("browser-cache", "Caché de navegadores", "Copias de páginas web de Chrome, Edge y Brave. Se vuelven a descargar cuando hagan falta.", "safe", [t.find("Users/Eliecer/AppData/Local/Google/Chrome/User Data/Default/Cache")]);
      add("dev-cache", "Caché de npm, pip y Yarn", "Paquetes descargados que se pueden volver a bajar.", "safe", [t.find("Users/Eliecer/AppData/Local/npm-cache")]);
      add("shader-cache", "Caché de sombreadores", "Los juegos la regeneran la próxima vez que se abren.", "safe", [t.find("Users/Eliecer/AppData/Local/NVIDIA/DXCache")]);
      const dl = t.find("Users/Eliecer/Downloads");
      const dlKids = dl !== undefined ? t.entries[dl].children : [];
      add("iso", "Imágenes ISO en Descargas", "Imágenes de discos de instalación. Revisa si todavía las necesitas.", "review", dlKids.filter((c) => extOf(t.entries[c].name) === "iso"));
      add("installers", "Instaladores en Descargas", "Archivos .exe y .msi que normalmente ya no hacen falta después de instalar.", "review", dlKids.filter((c) => ["exe", "msi"].includes(extOf(t.entries[c].name))));
      add("node-modules", "Carpetas node_modules", "Dependencias de proyectos. Se recuperan con npm install.", "review", [t.find("Users/Eliecer/Projects/tienda-online/node_modules"), t.find("Users/Eliecer/Projects/api-pagos/node_modules")]);
      add("hibernation", "Archivo de hibernación", "Desactivar la hibernación libera hiberfil.sys, pero quita la opción Hibernar del menú de apagado.", "careful", [t.find("hiberfil.sys")], "Abre una terminal como administrador y ejecuta: powercfg /hibernate off");
      return out;
    },
    async rescanFolder(root, id) {
      tree(root);
      await sleep(600);
      return id;
    },
    async openInExplorer() {},
    async moveToTrash(root, ids) {
      const t = tree(root);
      return ids.reduce((a, i) => a + t.remove(i), 0);
    },
  };
}
