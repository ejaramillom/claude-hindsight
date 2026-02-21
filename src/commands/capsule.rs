use anyhow::{Context, Result};
use hindsight::scc::Capsule;
use hindsight::scc::graph::GoalStatus;
use hindsight::scc::symtable::SymCategory;
use crate::storage::SessionIndex;
use crate::parser::parse_session;

pub fn create(session_id: String, output: String) -> Result<()> {
    let index = SessionIndex::new()?;
    let session_file = index.find_by_id(&session_id)?
        .context(format!("Session {} not found", session_id))?;
    
    let session = parse_session(&session_file.path)
        .context(format!("Failed to parse session {}", session_id))?;
    
    // Create new capsule v0 with no parent hash for now.
    let mut capsule = Capsule::new(session.session_id.clone(), vec![0u8; 32]);
    
    // Identity Axiom
    let root_sym = capsule.symtable.intern(&format!("Session: {}", session.session_id), SymCategory::Tag);
    capsule.state.add_axiom(root_sym);

    let mut active_goal_id: Option<u32> = None;

    // Heuristic Mapping
    for node in &session.nodes {
        if node.node_type == "user" {
            if let Some(msg) = &node.message {
                let text = msg.text_content();
                if !text.is_empty() {
                    // Axiom Detection: Look for definitive project-wide constraints
                    if text.starts_with("Always") || text.starts_with("Never") || text.contains("Language:") {
                         let sym = capsule.symtable.intern(&text, SymCategory::Tag);
                         capsule.state.add_axiom(sym);
                    } else {
                        // Default to Goal
                        let sym = capsule.symtable.intern(&text, SymCategory::Literal);
                        active_goal_id = Some(capsule.state.add_goal(sym, GoalStatus::Open));
                    }
                }
            }
        } else if node.node_type == "assistant" {
            if let Some(msg) = &node.message {
                // 1. Thinking Extraction
                if let Some(thinking) = &node.thinking {
                    if !thinking.is_empty() {
                        let sym = capsule.symtable.intern(&format!("[Reasoning] {}", thinking), SymCategory::Literal);
                        let did = capsule.state.add_decision(sym);
                        if let Some(gid) = active_goal_id {
                            if let Some(decision) = capsule.state.decisions.get_mut(did as usize) {
                                decision.goal_ids.push(gid);
                            }
                        }
                    }
                }

                // 2. Decision Extraction
                let text = msg.text_content();
                if !text.is_empty() {
                    let sym = capsule.symtable.intern(&text, SymCategory::Literal);
                    let did = capsule.state.add_decision(sym);
                    if let Some(gid) = active_goal_id {
                        if let Some(decision) = capsule.state.decisions.get_mut(did as usize) {
                            decision.goal_ids.push(gid);
                        }

                        // Heuristic Goal Completion
                        let lower_text = text.to_lowercase();
                        if lower_text.contains("done") || lower_text.contains("finished") || lower_text.contains("implemented") {
                            if let Some(goal) = capsule.state.goals.get_mut(gid as usize) {
                                goal.status = GoalStatus::Completed;
                            }
                        }
                    }

                    // 3. Pending Extraction (Questions or "Next Steps")
                    if text.contains("?") || text.contains("Next step") || text.contains("follow-up") {
                        // Extract the specific question/step if possible, or just the whole text for now
                        let sym = capsule.symtable.intern(&text, SymCategory::Literal);
                        capsule.state.add_pending(sym);
                    }
                }
            }
        } else if node.node_type == "tool_use" {
            if let Some(tool) = &node.tool_use {
                if let Some(file_path) = tool.input.get("file_path").and_then(|v| v.as_str()) {
                    let sym = capsule.symtable.intern(file_path, SymCategory::Resource);
                    capsule.state.add_resource(sym);
                }
            }
        }
    }

    // Commit to calculate hash and canonicalize.
    capsule.commit();
    
    // Always save to the global store for content-addressed recovery.
    let store_path = capsule.save_to_store().context("Failed to save to SCC store")?;
    println!("Capsule committed to store: {}", store_path.display());

    // Also save to explicit output if requested.
    capsule.save(&output).context("Failed to save capsule")?;
    println!("Capsule exported to: {}", output);
    println!("Header: {:?}", capsule.header);
    println!("Nodes: Axioms={}, Goals={}, Decisions={}, Resources={}, Pending={}", 
        capsule.state.axioms.len(), 
        capsule.state.goals.len(), 
        capsule.state.decisions.len(), 
        capsule.state.resources.len(),
        capsule.state.pending.len());
    
    Ok(())
}

pub fn hydrate(path: String) -> Result<()> {
    let capsule = Capsule::load(&path).context(format!("Failed to load capsule from {}", path))?;
    let prompt = capsule.rehydrate();
    println!("{}", prompt);
    Ok(())
}

pub fn status(path: String) -> Result<()> {
    let capsule = Capsule::load(&path).context(format!("Failed to load capsule from {}", path))?;
    println!("Capsule: {}", path);
    println!("----------------------------------------");
    println!("Version:    {}", capsule.header.version);
    println!("Root ID:    {}", capsule.header.root_id);
    println!("Created:    {}", chrono::DateTime::from_timestamp(capsule.header.timestamp as i64 / 1_000_000, 0).unwrap_or_default());
    println!("Hash:       {}", hex::encode(&capsule.header.hash));
    println!("Parent:     {}", hex::encode(&capsule.header.parent_hash));
    println!("----------------------------------------");
    println!("Axioms:     {}", capsule.state.axioms.len());
    println!("Goals:      {}", capsule.state.goals.len());
    println!("Decisions:  {}", capsule.state.decisions.len());
    println!("Resources:  {}", capsule.state.resources.len());
    println!("Pending:    {}", capsule.state.pending.len());
    Ok(())
}

pub fn diff(base_path: String, head_path: String) -> Result<()> {
    let base = Capsule::load(&base_path).context(format!("Failed to load base capsule from {}", base_path))?;
    let head = Capsule::load(&head_path).context(format!("Failed to load head capsule from {}", head_path))?;
    
    println!("Diff: {} -> {}", base_path, head_path);
    println!("----------------------------------------");
    
    // Compare Goals
    println!("## GOALS");
    for (i, h_goal) in head.state.goals.iter().enumerate() {
        if let Some(b_goal) = base.state.goals.get(i) {
            if h_goal.status != b_goal.status {
                let h_text = head.symtable.get(h_goal.text_sym).unwrap_or("unknown");
                println!("[STATUS] G{}: {:?} -> {:?}", i, b_goal.status, h_goal.status);
                println!("         {}", h_text);
            }
        } else {
            let h_text = head.symtable.get(h_goal.text_sym).unwrap_or("unknown");
            println!("[NEW]    G{}: {}", i, h_text);
        }
    }
    
    // Compare Pending
    println!("\n## PENDING");
    let base_pending: std::collections::HashSet<_> = base.state.pending.iter().map(|p| p.text_sym).collect();
    for p in &head.state.pending {
        if !base_pending.contains(&p.text_sym) {
            let h_text = head.symtable.get(p.text_sym).unwrap_or("unknown");
            println!("[NEW]    ? {}", h_text);
        }
    }
    
    Ok(())
}

pub fn merge(base_path: String, head_a_path: String, head_b_path: String, output: String) -> Result<()> {
    let base = Capsule::load(&base_path).context(format!("Failed to load base capsule from {}", base_path))?;
    let head_a = Capsule::load(&head_a_path).context(format!("Failed to load head-a capsule from {}", head_a_path))?;
    let head_b = Capsule::load(&head_b_path).context(format!("Failed to load head-b capsule from {}", head_b_path))?;
    
    let merged = Capsule::merge(&base, &head_a, &head_b).context("Failed to merge capsules")?;
    
    merged.save(&output).context("Failed to save merged capsule")?;
    println!("Capsules merged successfully into: {}", output);
    println!("New Hash: {}", hex::encode(&merged.header.hash));
    
    Ok(())
}
