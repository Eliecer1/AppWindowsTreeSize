use serde::Serialize;

/// Grupo de archivos que la interfaz usa para colorear y resumir el espacio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Video,
    Image,
    App,
    Archive,
    System,
    Docs,
    Code,
    Cache,
    Other,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Video,
        Category::Image,
        Category::App,
        Category::Archive,
        Category::System,
        Category::Docs,
        Category::Code,
        Category::Cache,
        Category::Other,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_ext(ext: &str) -> Category {
        match ext {
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "m4v" | "flv" | "m2ts" => Category::Video,
            "jpg" | "jpeg" | "png" | "heic" | "heif" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "gif"
            | "webp" | "bmp" | "tif" | "tiff" | "psd" => Category::Image,
            "exe" | "dll" | "pak" | "archive" | "vpk" | "bin" | "asar" | "node" | "so" | "ucas" | "utoc" => {
                Category::App
            }
            "iso" | "zip" | "rar" | "7z" | "msi" | "msp" | "cab" | "tgz" | "gz" | "tar" | "xz" | "img" => {
                Category::Archive
            }
            "sys" | "vhd" | "vhdx" | "etl" | "mum" | "cat" | "manifest" | "evtx" | "wim" | "esd" => Category::System,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "odt" | "csv" | "epub" => Category::Docs,
            "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "json" | "map" | "rs" | "py" | "java" | "cs" | "cpp"
            | "h" | "go" | "pdb" | "lib" | "obj" | "rlib" | "pack" | "idx" => Category::Code,
            "tmp" | "temp" | "log" | "cache" | "nvph" | "dmp" | "old" | "bak" => Category::Cache,
            _ => Category::Other,
        }
    }
}

/// Contexto heredado de las carpetas que rodean a un archivo.
/// Todo lo que vive dentro de `node_modules` es código aunque su extensión diga otra cosa,
/// y todo lo que vive dentro de una carpeta de caché es caché.
#[derive(Clone, Copy, Default, Debug)]
pub struct Context {
    pub cache: bool,
    pub code: bool,
}

impl Context {
    pub fn enter(self, dir_name: &str) -> Context {
        let lower = dir_name.to_ascii_lowercase();
        let cache = self.cache
            || matches!(
                lower.as_str(),
                "temp" | "tmp" | "cache" | "cache_data" | "code cache" | "gpucache" | "_cacache" | "dxcache"
                    | "inetcache" | "shadercache" | "htmlcache" | "crashdumps" | "nv_cache"
            );
        let code = self.code || matches!(lower.as_str(), "node_modules" | ".git" | "target" | ".venv" | "__pycache__");
        Context { cache, code }
    }

    pub fn categorize(self, ext: &str) -> Category {
        if self.cache {
            Category::Cache
        } else if self.code {
            Category::Code
        } else {
            Category::from_ext(ext)
        }
    }
}

/// Extensión en minúsculas, sin punto. Vacía si no tiene.
pub fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 10 => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(extension("Video.MKV"), "mkv");
        assert_eq!(extension(".gitignore"), "");
        assert_eq!(extension("f_00a1b2"), "");
        assert_eq!(extension("archivo.tar.gz"), "gz");
    }

    #[test]
    fn context_overrides_extension() {
        let ctx = Context::default().enter("AppData").enter("Temp");
        assert_eq!(ctx.categorize("png"), Category::Cache);
        let ctx = Context::default().enter("node_modules");
        assert_eq!(ctx.categorize("md"), Category::Code);
        assert_eq!(Context::default().categorize("iso"), Category::Archive);
    }
}
