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
    /// Get the default SCC store directory (~/.scc/store).
    pub fn get_store_dir() -> Result<std::path::PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
        let store = home.join(".scc").join("store");
        if !store.exists() {
            std::fs::create_dir_all(&store).context("Failed to create SCC store directory")?;
        }
        Ok(store)
    }

    /// Save the capsule to the default store using its hash as the filename.
    pub fn save_to_store(&self) -> Result<std::path::PathBuf> {
        let hash_hex = hex::encode(&self.header.hash);
        let store_dir = Self::get_store_dir()?;
        let path = store_dir.join(format!("{}.scc", hash_hex));
        self.save(&path)?;
        Ok(path)
    }

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
        
        for pending in &self.state.pending {
            if !self.symtable.contains(pending.text_sym) {
                return Err(anyhow!("Pending references unknown SymID: {}", pending.text_sym));
            }
        }
        
        Ok(())
    }

    /// Perform a 3-way merge of two diverging capsules from a common base.
    pub fn merge(base: &Capsule, head_a: &Capsule, head_b: &Capsule) -> Result<Capsule> {
        if head_a.header.root_id != base.header.root_id || head_b.header.root_id != base.header.root_id {
            return Err(anyhow!("Cannot merge capsules with different Root IDs"));
        }

        let mut merged = Capsule::new(base.header.root_id.clone(), head_a.header.hash.clone());
        
        // 1. Merge Symbol Tables.
        merged.symtable.merge(&base.symtable);
        merged.symtable.merge(&head_a.symtable);
        merged.symtable.merge(&head_b.symtable);
        merged.symtable.canonicalize();

        // 2. Union of Axioms, Resources, and Pending.
        let mut all_axioms: std::collections::HashSet<u64> = base.state.axioms.iter().map(|n| n.text_sym).collect();
        all_axioms.extend(head_a.state.axioms.iter().map(|n| n.text_sym));
        all_axioms.extend(head_b.state.axioms.iter().map(|n| n.text_sym));
        for sym in all_axioms { merged.state.add_axiom(sym); }

        let mut all_resources: std::collections::HashSet<u64> = base.state.resources.iter().map(|n| n.text_sym).collect();
        all_resources.extend(head_a.state.resources.iter().map(|n| n.text_sym));
        all_resources.extend(head_b.state.resources.iter().map(|n| n.text_sym));
        for sym in all_resources { merged.state.add_resource(sym); }

        let mut all_pending: std::collections::HashSet<u64> = base.state.pending.iter().map(|n| n.text_sym).collect();
        all_pending.extend(head_a.state.pending.iter().map(|n| n.text_sym));
        all_pending.extend(head_b.state.pending.iter().map(|n| n.text_sym));
        for sym in all_pending { merged.state.add_pending(sym); }

        // 3. Merge Goals.
        let mut all_goal_syms: std::collections::HashSet<u64> = base.state.goals.iter().map(|n| n.text_sym).collect();
        all_goal_syms.extend(head_a.state.goals.iter().map(|n| n.text_sym));
        all_goal_syms.extend(head_b.state.goals.iter().map(|n| n.text_sym));

        for sym in all_goal_syms {
            let g_base = base.state.find_goal_by_sym(sym);
            let g_a = head_a.state.find_goal_by_sym(sym);
            let g_b = head_b.state.find_goal_by_sym(sym);

            let status = match (g_base, g_a, g_b) {
                (Some(b), Some(a), Some(h)) => {
                    if a.status == h.status { a.status }
                    else if a.status == b.status { h.status }
                    else if h.status == b.status { a.status }
                    else {
                        // Strict Conflict Resolution from Step 7 plan.
                        if (a.status == GoalStatus::Completed && (h.status == GoalStatus::Blocked || h.status == GoalStatus::Deprecated)) ||
                           (h.status == GoalStatus::Completed && (a.status == GoalStatus::Blocked || a.status == GoalStatus::Deprecated)) {
                            let text = merged.symtable.get(sym).unwrap_or("unknown");
                            return Err(anyhow!("Conflict in Goal status for '{}': {:?} vs {:?}", text, a.status, h.status));
                        }
                        std::cmp::max(a.status, h.status)
                    }
                }
                (_, Some(a), Some(h)) => std::cmp::max(a.status, h.status),
                (_, Some(a), None) => a.status,
                (_, None, Some(h)) => h.status,
                (Some(b), None, None) => b.status, // Goal was removed? (Not supported yet, but for safety).
                _ => GoalStatus::Open,
            };

            let merged_goal = crate::scc::graph::Goal {
                id: 0, // Will be re-indexed.
                text_sym: sym,
                status,
                parent_ids: Vec::new(),
            };

            // Merge Parent IDs (Collect text_syms first).
            let mut parent_syms: std::collections::HashSet<u64> = Vec::new().into_iter().collect();
            if let Some(b) = g_base { parent_syms.extend(b.parent_ids.iter().map(|&pid| base.state.goals[pid as usize].text_sym)); }
            if let Some(a) = g_a { parent_syms.extend(a.parent_ids.iter().map(|&pid| head_a.state.goals[pid as usize].text_sym)); }
            if let Some(h) = g_b { parent_syms.extend(h.parent_ids.iter().map(|&pid| head_b.state.goals[pid as usize].text_sym)); }

            // Store parent_syms temporarily in parent_ids (mapped back during canonicalization).
            // Wait, parent_ids is Vec<u32>. This is not enough for u64 text_sym.
            // Let's use a temporary representation or rely on the fact that goals are added after their parents.
            // Actually, the easiest is to add goals and then use find_goal_by_sym for parents during re-indexing.
            merged.state.goals.push(merged_goal);
        }

        // Re-link goal parents using text_sym.
        let mut goal_copies = merged.state.goals.clone();
        for (i, goal) in goal_copies.iter_mut().enumerate() {
            let mut parent_syms: std::collections::HashSet<u64> = std::collections::HashSet::new();
            if let Some(b) = base.state.find_goal_by_sym(goal.text_sym) { parent_syms.extend(b.parent_ids.iter().map(|&pid| base.state.goals[pid as usize].text_sym)); }
            if let Some(a) = head_a.state.find_goal_by_sym(goal.text_sym) { parent_syms.extend(a.parent_ids.iter().map(|&pid| head_a.state.goals[pid as usize].text_sym)); }
            if let Some(h) = head_b.state.find_goal_by_sym(goal.text_sym) { parent_syms.extend(h.parent_ids.iter().map(|&pid| head_b.state.goals[pid as usize].text_sym)); }
            
            for psym in parent_syms {
                if let Some(pos) = merged.state.goals.iter().position(|g| g.text_sym == psym) {
                    goal.parent_ids.push(pos as u32);
                }
            }
            goal.id = i as u32;
        }
        merged.state.goals = goal_copies;

        // 4. Merge Decisions.
        let mut all_decision_syms: std::collections::HashSet<u64> = base.state.decisions.iter().map(|n| n.text_sym).collect();
        all_decision_syms.extend(head_a.state.decisions.iter().map(|n| n.text_sym));
        all_decision_syms.extend(head_b.state.decisions.iter().map(|n| n.text_sym));

        for sym in all_decision_syms {
            let d_base = base.state.find_decision_by_sym(sym);
            let d_a = head_a.state.find_decision_by_sym(sym);
            let d_b = head_b.state.find_decision_by_sym(sym);

            let mut addressed_goal_syms: std::collections::HashSet<u64> = std::collections::HashSet::new();
            if let Some(d) = d_base { addressed_goal_syms.extend(d.goal_ids.iter().map(|&gid| base.state.goals[gid as usize].text_sym)); }
            if let Some(d) = d_a { addressed_goal_syms.extend(d.goal_ids.iter().map(|&gid| head_a.state.goals[gid as usize].text_sym)); }
            if let Some(d) = d_b { addressed_goal_syms.extend(d.goal_ids.iter().map(|&gid| head_b.state.goals[gid as usize].text_sym)); }

            let mut decision = crate::scc::graph::Decision {
                id: merged.state.decisions.len() as u32,
                text_sym: sym,
                goal_ids: Vec::new(),
            };

            for gsym in addressed_goal_syms {
                if let Some(pos) = merged.state.goals.iter().position(|g| g.text_sym == gsym) {
                    decision.goal_ids.push(pos as u32);
                }
            }
            merged.state.decisions.push(decision);
        }

        // Final Canonicalization.
        merged.commit();
        Ok(merged)
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
            output.push_str("\n");
        }

        // Pending
        if !self.state.pending.is_empty() {
            output.push_str("## PENDING\n");
            for pending in &self.state.pending {
                if let Some(text) = self.symtable.get(pending.text_sym) {
                    output.push_str(&format!("? {}\n", text));
                }
            }
        }

        output.push_str("</scc_context>");
        output
    }
}
