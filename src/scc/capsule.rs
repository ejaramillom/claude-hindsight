use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use anyhow::{Context, Result, anyhow};
use blake3::Hasher;

use crate::scc::symtable::SymTable;
use crate::scc::graph::{State, GoalStatus};

/// Metadata defining the identity and immutability of the capsule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    /// Schema version (v0 = 0).
    pub version: u32,
    /// Unix micros (UTC).
    pub timestamp: u64,
    /// Blake3 hash of (SymbolTable + State).
    pub hash: Vec<u8>,
    /// Link to predecessor (Lineage).
    pub parent_hash: Vec<u8>,
    /// Unique Session/Project UUID.
    pub root_id: String,
}

/// A Semantic Context Capsule (SCC) - a static, immutable snapshot of a semantic context graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capsule {
    pub header: Header,
    pub symtable: SymTable,
    pub state: State,
}

impl Capsule {
    /// Create a new empty capsule.
    pub fn new(root_id: String, parent_hash: Vec<u8>) -> Self {
        Self {
            header: Header {
                version: 0,
                timestamp: chrono::Utc::now().timestamp_micros() as u64,
                hash: Vec::new(),
                parent_hash,
                root_id,
            },
            symtable: SymTable::new(),
            state: State::new(),
        }
    }

    /// Calculate the Blake3 hash of the (SymbolTable + State).
    pub fn calculate_hash(&self) -> Vec<u8> {
        let mut hasher = Hasher::new();
        // Deterministically serialize symtable and state to byte streams.
        // For hashing, we assume the SymTable and State are already canonicalized.
        let sym_data = serde_json::to_vec(&self.symtable).unwrap();
        let state_data = serde_json::to_vec(&self.state).unwrap();
        
        hasher.update(&sym_data);
        hasher.update(&state_data);
        hasher.finalize().as_bytes().to_vec()
    }

    /// Commit the capsule by calculating and updating the hash in the header.
    pub fn commit(&mut self) {
        // First canonicalize everything.
        self.symtable.canonicalize();
        self.state.canonicalize();
        
        // Update hash.
        let hash = self.calculate_hash();
        self.header.hash = hash;
    }

    /// Save the capsule to a file (using Zstd compression).
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_vec(self).context("Failed to serialize capsule")?;
        let compressed = zstd::encode_all(&json[..], 3).context("Failed to compress capsule")?;
        
        let mut file = File::create(path).context("Failed to create capsule file")?;
        file.write_all(&compressed).context("Failed to write capsule file")?;
        
        Ok(())
    }

    /// Load a capsule from a file and perform initial validation.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path).context("Failed to open capsule file")?;
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed).context("Failed to read capsule file")?;
        
        let decompressed = zstd::decode_all(&compressed[..]).context("Failed to decompress capsule")?;
        let mut capsule: Self = serde_json::from_slice(&decompressed).context("Failed to deserialize capsule")?;
        
        // Rebuild the reverse map of the symbol table.
        capsule.symtable.rebuild_reverse_map();
        
        // Validate the hash.
        let calculated_hash = capsule.calculate_hash();
        if calculated_hash != capsule.header.hash {
            return Err(anyhow!("Capsule hash mismatch. Expected {:?}, got {:?}", capsule.header.hash, calculated_hash));
        }
        
        // Perform structural validation.
        capsule.validate().context("Capsule structural validation failed")?;
        
        Ok(capsule)
    }

    /// Validate the integrity of the semantic graph.
    pub fn validate(&self) -> Result<()> {
        // 1. Check SymID existence for all nodes.
        for axiom in &self.state.axioms {
            if !self.symtable.contains(axiom.text_sym) {
                return Err(anyhow!("Axiom references unknown SymID: {}", axiom.text_sym));
            }
        }
        
        for (i, goal) in self.state.goals.iter().enumerate() {
            if !self.symtable.contains(goal.text_sym) {
                return Err(anyhow!("Goal #{} references unknown SymID: {}", i, goal.text_sym));
            }
            if goal.id as usize != i {
                return Err(anyhow!("Goal #{} has invalid positional ID: {}", i, goal.id));
            }
            // 2. Check Parent IDs (no cycles - must point to lower indices).
            for &pid in &goal.parent_ids {
                if pid >= goal.id {
                    return Err(anyhow!("Goal #{} references invalid/forward parent ID: {}", i, pid));
                }
                if pid as usize >= self.state.goals.len() {
                    return Err(anyhow!("Goal #{} references non-existent parent ID: {}", i, pid));
                }
            }
        }
        
        for (i, decision) in self.state.decisions.iter().enumerate() {
            if !self.symtable.contains(decision.text_sym) {
                return Err(anyhow!("Decision #{} references unknown SymID: {}", i, decision.text_sym));
            }
            // Check Goals linked to this decision.
            for &gid in &decision.goal_ids {
                if gid as usize >= self.state.goals.len() {
                    return Err(anyhow!("Decision #{} references unknown GoalID: {}", i, gid));
                }
            }
        }
        
        for resource in &self.state.resources {
            if !self.symtable.contains(resource.text_sym) {
                return Err(anyhow!("Resource references unknown SymID: {}", resource.text_sym));
            }
        }
        
        Ok(())
    }

    /// Return a token-efficient, rehydrated prompt from the capsule.
    pub fn rehydrate(&self) -> String {
        let mut output = String::new();
        output.push_str("<scc_context version=\"0.0\">\n\n");

        // Axioms
        if !self.state.axioms.is_empty() {
            output.push_str("## AXIOMS\n");
            for axiom in &self.state.axioms {
                if let Some(text) = self.symtable.get(axiom.text_sym) {
                    output.push_str(&format!("! {}\n", text));
                }
            }
            output.push_str("\n");
        }

        // Goals
        if !self.state.goals.is_empty() {
            output.push_str("## GOALS\n");
            for goal in &self.state.goals {
                if let Some(text) = self.symtable.get(goal.text_sym) {
                    let status_char = match goal.status {
                        GoalStatus::Open => " ",
                        GoalStatus::Completed => "x",
                        GoalStatus::Blocked => "!",
                        GoalStatus::Deprecated => "-",
                    };
                    output.push_str(&format!("[{}] G{}: {}\n", status_char, goal.id, text));
                    
                    // Show dependencies.
                    for &pid in &goal.parent_ids {
                        output.push_str(&format!("    ^-- depends on G{}\n", pid));
                    }
                }
            }
            output.push_str("\n");
        }

        // Decisions
        if !self.state.decisions.is_empty() {
            output.push_str("## DECISIONS\n");
            for (id, decision) in self.state.decisions.iter().enumerate() {
                if let Some(text) = self.symtable.get(decision.text_sym) {
                    output.push_str(&format!("* D{}: {}\n", id, text));
                    // Show goals addressed.
                    for &gid in &decision.goal_ids {
                        output.push_str(&format!("    ^-- addresses G{}\n", gid));
                    }
                }
            }
            output.push_str("\n");
        }

        // Resources
        if !self.state.resources.is_empty() {
            output.push_str("## RESOURCES\n");
            for res in &self.state.resources {
                if let Some(text) = self.symtable.get(res.text_sym) {
                    output.push_str(&format!("@ {}\n", text));
                }
            }
        }

        output.push_str("</scc_context>");
        output
    }
}
