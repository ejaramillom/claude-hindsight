#[cfg(test)]
mod tests {
    use crate::scc::Capsule;
    use crate::scc::graph::GoalStatus;
    use crate::scc::symtable::SymCategory;
    use tempfile::tempdir;

    #[test]
    fn test_capsule_roundtrip() {
        let root_id = "test-session-uuid".to_string();
        let parent_hash = vec![0u8; 32];
        let mut capsule = Capsule::new(root_id, parent_hash);
        
        // Add Axioms
        let lang_sym = capsule.symtable.intern("Language: Rust", SymCategory::Tag);
        capsule.state.add_axiom(lang_sym);
        
        // Add Goal
        let auth_sym = capsule.symtable.intern("Implement User Auth", SymCategory::Literal);
        let goal_id = capsule.state.add_goal(auth_sym, GoalStatus::Open);
        
        // Add Decision
        let jwt_sym = capsule.symtable.intern("Use JWT for sessions", SymCategory::Literal);
        let _decision_id = capsule.state.add_decision(jwt_sym);
        
        // Add Pending
        let next_sym = capsule.symtable.intern("Review Step 5", SymCategory::Literal);
        capsule.state.add_pending(next_sym);
        
        // Commit (calculates hash)
        capsule.commit();
        
        // Save and Load
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.scc");
        capsule.save(&path).unwrap();
        
        let loaded = Capsule::load(&path).unwrap();
        
        assert_eq!(loaded.header.version, 0);
        assert_eq!(loaded.state.goals.len(), 1);
        assert_eq!(loaded.state.pending.len(), 1);
        assert_eq!(loaded.symtable.get(lang_sym), Some("Language: Rust"));
        assert_eq!(loaded.symtable.get(next_sym), Some("Review Step 5"));
        assert_eq!(loaded.header.root_id, "test-session-uuid");
        assert_eq!(loaded.header.parent_hash, vec![0u8; 32]);
        
        let prompt = loaded.rehydrate();
        assert!(prompt.contains("## AXIOMS"));
        assert!(prompt.contains("! Language: Rust"));
        assert!(prompt.contains("## GOALS"));
        assert!(prompt.contains("[ ] G0: Implement User Auth"));
        assert!(prompt.contains("## DECISIONS"));
        assert!(prompt.contains("* D0: Use JWT for sessions"));
        assert!(prompt.contains("## PENDING"));
        assert!(prompt.contains("? Review Step 5"));
    }
}
