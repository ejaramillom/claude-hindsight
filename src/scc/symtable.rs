use std::collections::HashMap;
use seahash::hash;
use serde::{Serialize, Deserialize};

/// Symbols are partitioned by usage to prevent collision and aid rehydration heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymCategory {
    Literal,
    Resource,
    Tag,
}

/// The Symbol Table is the singular source of truth for all literal data within a capsule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymTable {
    /// Registry: A map of Global ID (Hash64) -> String.
    /// Stored as a sorted list for deterministic serialization.
    pub symbols: Vec<(u64, String)>,
    
    /// Reverse mapping for quick lookups during interning (not serialized).
    #[serde(skip)]
    reverse_map: HashMap<String, u64>,
}

impl Default for SymTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymTable {
    /// Create a new empty Symbol Table.
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            reverse_map: HashMap::new(),
        }
    }

    /// Intern a string into the table, returning its Global ID (Hash64).
    pub fn intern(&mut self, text: &str, _category: SymCategory) -> u64 {
        let normalized = text.trim();
        
        if let Some(&id) = self.reverse_map.get(normalized) {
            return id;
        }
        
        let id = hash(normalized.as_bytes());
        self.symbols.push((id, normalized.to_string()));
        self.reverse_map.insert(normalized.to_string(), id);
        id
    }

    /// Look up a symbol by its Global ID.
    pub fn get(&self, id: u64) -> Option<&str> {
        // Since we store as a sorted list, we can use binary search or re-build the reverse map.
        // For lookup, we assume the map is rebuilt after load.
        self.symbols.iter().find(|(k, _)| *k == id).map(|(_, v)| v.as_str())
    }

    /// Rebuild the reverse map after loading from file.
    pub fn rebuild_reverse_map(&mut self) {
        self.reverse_map.clear();
        for (id, text) in &self.symbols {
            self.reverse_map.insert(text.clone(), *id);
        }
    }

    /// Canonicalize the table by sorting by value.
    pub fn canonicalize(&mut self) {
        self.symbols.sort_by(|a, b| a.1.cmp(&b.1));
    }

    /// Check if a symbol ID exists.
    pub fn contains(&self, id: u64) -> bool {
        self.symbols.iter().any(|(k, _)| *k == id)
    }

    /// Merge another SymTable into this one.
    pub fn merge(&mut self, other: &SymTable) {
        for (id, text) in &other.symbols {
            if !self.contains(*id) {
                self.symbols.push((*id, text.clone()));
                self.reverse_map.insert(text.clone(), *id);
            }
        }
    }
}
