//! Comparar dos análisis del mismo disco: qué creció, qué se achicó, qué apareció y qué se borró.
//!
//! El resultado es un árbol pequeño: solo entran los elementos que cambiaron al menos `min` bytes,
//! y de cada carpeta como mucho los `MAX_CHILDREN` cambios más grandes.

use crate::{NodeId, Tree};
use serde::Serialize;
use std::collections::HashMap;

const MAX_CHILDREN: usize = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub name: String,
    pub is_dir: bool,
    /// Espacio en disco en el análisis anterior (0 si no existía).
    pub old: u64,
    /// Espacio en disco ahora (0 si se borró).
    pub new: u64,
    pub existed: bool,
    pub exists: bool,
    pub children: Vec<Change>,
}

impl Change {
    pub fn delta(&self) -> i128 {
        self.new as i128 - self.old as i128
    }
}

/// Compara el análisis anterior `old` con el actual `new`.
pub fn diff(old: &Tree, new: &Tree, min: u64) -> Change {
    let mut c = node(old, new, Some(Tree::ROOT), Some(Tree::ROOT), min);
    c.name = new.root_path().to_string_lossy().into_owned();
    c
}

fn node(old: &Tree, new: &Tree, o: Option<NodeId>, n: Option<NodeId>, min: u64) -> Change {
    let (name, is_dir) = match (n, o) {
        (Some(n), _) => (new.name(n).to_string(), new.is_dir(n)),
        (None, Some(o)) => (old.name(o).to_string(), old.is_dir(o)),
        (None, None) => unreachable!(),
    };
    let mut c = Change {
        name,
        is_dir,
        old: o.map_or(0, |o| old.size(o)),
        new: n.map_or(0, |n| new.size(n)),
        existed: o.is_some(),
        exists: n.is_some(),
        children: Vec::new(),
    };
    if !is_dir {
        return c;
    }
    let old_kids: HashMap<String, NodeId> =
        o.map(|o| old.kids(o).iter().map(|k| (old.name(*k).to_lowercase(), *k)).collect()).unwrap_or_default();
    let mut matched = std::collections::HashSet::new();
    let mut pairs: Vec<(Option<NodeId>, Option<NodeId>, u64)> = Vec::new();
    if let Some(n) = n {
        for &k in new.kids(n) {
            let ok = old_kids.get(&new.name(k).to_lowercase()).copied().filter(|ok| old.is_dir(*ok) == new.is_dir(k));
            if let Some(ok) = ok {
                matched.insert(ok);
            }
            let d = new.size(k).abs_diff(ok.map_or(0, |ok| old.size(ok)));
            if d >= min {
                pairs.push((ok, Some(k), d));
            }
        }
    }
    for &ok in old_kids.values() {
        if !matched.contains(&ok) && old.size(ok) >= min {
            pairs.push((Some(ok), None, old.size(ok)));
        }
    }
    pairs.sort_unstable_by(|a, b| b.2.cmp(&a.2));
    pairs.truncate(MAX_CHILDREN);
    c.children = pairs.into_iter().map(|(ok, nk, _)| node(old, new, ok, nk, min)).collect();
    c
}
