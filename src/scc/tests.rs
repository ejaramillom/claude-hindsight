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

    #[test]
    fn test_capsule_merge_3way() {
        use crate::scc::symtable::SymCategory;
        use crate::scc::graph::GoalStatus;

        let root_id = "merge-test".to_string();
        
        // 1. Base Capsule
        let mut base = Capsule::new(root_id.clone(), vec![0u8; 32]);
        let g1_sym = base.symtable.intern("Goal 1", SymCategory::Literal);
        base.state.add_goal(g1_sym, GoalStatus::Open);
        base.commit();

        // 2. Head A: Goal 1 Completed + New Goal A
        let mut head_a = base.clone();
        head_a.header.parent_hash = base.header.hash.clone();
        if let Some(g) = head_a.state.goals.get_mut(0) { g.status = GoalStatus::Completed; }
        let ga_sym = head_a.symtable.intern("Goal A", SymCategory::Literal);
        head_a.state.add_goal(ga_sym, GoalStatus::Open);
        head_a.commit();

        // 3. Head B: Goal 1 still Open + New Goal B
        let mut head_b = base.clone();
        head_b.header.parent_hash = base.header.hash.clone();
        let gb_sym = head_b.symtable.intern("Goal B", SymCategory::Literal);
        head_b.state.add_goal(gb_sym, GoalStatus::Open);
        head_b.commit();

        // 4. Merge
        let merged = Capsule::merge(&base, &head_a, &head_b).unwrap();
        
        // Assertions
        assert_eq!(merged.state.goals.len(), 3);
        
        // Goal 1 should be Completed (from head_a)
        let g1 = merged.state.find_goal_by_sym(g1_sym).unwrap();
        assert_eq!(g1.status, GoalStatus::Completed);
        
        // Goal A and B should both be present
        assert!(merged.state.find_goal_by_sym(ga_sym).is_some());
        assert!(merged.state.find_goal_by_sym(gb_sym).is_some());
        
        // Symbol table should be unified
        assert_eq!(merged.symtable.get(ga_sym), Some("Goal A"));
        assert_eq!(merged.symtable.get(gb_sym), Some("Goal B"));
    }

    #[test]
    fn test_capsule_merge_conflict() {
        use crate::scc::symtable::SymCategory;
        use crate::scc::graph::GoalStatus;

        let root_id = "conflict-test".to_string();
        let mut base = Capsule::new(root_id.clone(), vec![0u8; 32]);
        let g1_sym = base.symtable.intern("Goal 1", SymCategory::Literal);
        base.state.add_goal(g1_sym, GoalStatus::Open);
        base.commit();

        let mut head_a = base.clone();
        if let Some(g) = head_a.state.goals.get_mut(0) { g.status = GoalStatus::Completed; }
        head_a.commit();

        let mut head_b = base.clone();
        if let Some(g) = head_b.state.goals.get_mut(0) { g.status = GoalStatus::Blocked; }
        head_b.commit();

        let result = Capsule::merge(&base, &head_a, &head_b);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Conflict"));
    }
}
