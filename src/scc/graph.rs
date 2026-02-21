use serde::{Serialize, Deserialize};

/// Status for Goal nodes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum GoalStatus {
    Open = 0,
    Completed = 1,
    Blocked = 2,
    Deprecated = 3,
}

/// A foundation fact or constraint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Axiom {
    /// Symbol ID for the content.
    pub text_sym: u64,
}

/// An active objective.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Goal {
    /// Unique ID within the state (positional index).
    pub id: u32,
    /// Symbol ID for the content.
    pub text_sym: u64,
    /// Current status of the goal.
    pub status: GoalStatus,
    /// Dependencies (references to other node IDs).
    pub parent_ids: Vec<u32>,
}

/// A resolved architectural choice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    /// Unique ID within the state (positional index).
    pub id: u32,
    /// Symbol ID for the content.
    pub text_sym: u64,
    /// Goals this decision addresses (Node IDs).
    pub goal_ids: Vec<u32>,
}

/// An external pointer (file, URL, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    /// Symbol ID for the content (the path or identifier).
    pub text_sym: u64,
}

/// An unresolved question or task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pending {
    /// Symbol ID for the content.
    pub text_sym: u64,
}

/// The Compiled Semantic Graph (State).
/// It consists of flat lists of typed nodes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub axioms: Vec<Axiom>,
    pub goals: Vec<Goal>,
    pub decisions: Vec<Decision>,
    pub resources: Vec<Resource>,
    pub pending: Vec<Pending>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an axiom and return its position.
    pub fn add_axiom(&mut self, text_sym: u64) -> u32 {
        let id = self.axioms.len() as u32;
        self.axioms.push(Axiom { text_sym });
        id
    }

    /// Add a goal and return its ID.
    pub fn add_goal(&mut self, text_sym: u64, status: GoalStatus) -> u32 {
        let id = self.goals.len() as u32;
        self.goals.push(Goal {
            id,
            text_sym,
            status,
            parent_ids: Vec::new(),
        });
        id
    }

    /// Add a decision and return its ID.
    pub fn add_decision(&mut self, text_sym: u64) -> u32 {
        let id = self.decisions.len() as u32;
        self.decisions.push(Decision {
            id,
            text_sym,
            goal_ids: Vec::new(),
        });
        id
    }

    /// Add a resource and return its position.
    pub fn add_resource(&mut self, text_sym: u64) -> u32 {
        let id = self.resources.len() as u32;
        self.resources.push(Resource { text_sym });
        id
    }

    /// Add a pending node and return its position.
    pub fn add_pending(&mut self, text_sym: u64) -> u32 {
        let id = self.pending.len() as u32;
        self.pending.push(Pending { text_sym });
        id
    }

    /// Find a goal by its text_sym.
    pub fn find_goal_by_sym(&self, text_sym: u64) -> Option<&Goal> {
        self.goals.iter().find(|g| g.text_sym == text_sym)
    }

    /// Find a decision by its text_sym.
    pub fn find_decision_by_sym(&self, text_sym: u64) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.text_sym == text_sym)
    }

    /// Canonicalize the state by sorting all nodes and re-mapping IDs to ensure 
    /// that identical semantic content results in an identical byte representation.
    pub fn canonicalize(&mut self) {
        self.axioms.sort_by_key(|n| n.text_sym);
        self.resources.sort_by_key(|n| n.text_sym);
        self.pending.sort_by_key(|n| n.text_sym);

        // 1. Sort Goals by content (SymID)
        let mut goal_map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut sorted_goals = self.goals.clone();
        // We sort by text_sym to be deterministic.
        sorted_goals.sort_by_key(|g| g.text_sym);

        for (new_id, goal) in sorted_goals.iter_mut().enumerate() {
            goal_map.insert(goal.id, new_id as u32);
            goal.id = new_id as u32;
        }

        // 2. Update Goal parent_ids with new mappings
        for goal in &mut sorted_goals {
            for pid in &mut goal.parent_ids {
                if let Some(&new_pid) = goal_map.get(pid) {
                    *pid = new_pid;
                }
            }
            goal.parent_ids.sort();
        }
        self.goals = sorted_goals;

        // 3. Sort Decisions and update goal_ids
        self.decisions.sort_by_key(|d| d.text_sym);
        for (new_id, decision) in self.decisions.iter_mut().enumerate() {
            decision.id = new_id as u32;
            for gid in &mut decision.goal_ids {
                if let Some(&new_gid) = goal_map.get(gid) {
                    *gid = new_gid;
                }
            }
            decision.goal_ids.sort();
        }
    }
}
