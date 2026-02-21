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

    // Heuristic Mapping
    for node in &session.nodes {
        if node.node_type == "user" {
            if let Some(msg) = &node.message {
                let text = msg.text_content();
                if !text.is_empty() {
                    let sym = capsule.symtable.intern(&text, SymCategory::Literal);
                    capsule.state.add_goal(sym, GoalStatus::Open);
                }
            }
        } else if node.node_type == "assistant" {
            if let Some(msg) = &node.message {
                let text = msg.text_content();
                if !text.is_empty() {
                    let sym = capsule.symtable.intern(&text, SymCategory::Literal);
                    capsule.state.add_decision(sym);
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
    
    capsule.save(&output).context("Failed to save capsule")?;
    println!("Capsule created at: {}", output);
    println!("Header: {:?}", capsule.header);
    println!("Nodes: {}", capsule.state.axioms.len() + capsule.state.goals.len() + capsule.state.decisions.len() + capsule.state.resources.len());
    
    Ok(())
}

pub fn hydrate(path: String) -> Result<()> {
    let capsule = Capsule::load(&path).context(format!("Failed to load capsule from {}", path))?;
    let prompt = capsule.rehydrate();
    println!("{}", prompt);
    Ok(())
}
