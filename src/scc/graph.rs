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

/// The Compiled Semantic Graph (State).
/// It consists of flat lists of typed nodes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    pub axioms: Vec<Axiom>,
    pub goals: Vec<Goal>,
    pub decisions: Vec<Decision>,
    pub resources: Vec<Resource>,
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

    /// Canonicalize the state by sorting nodes.
    /// Note: This is simplified; real canonicalization should maintain ID stability or re-index.
    pub fn canonicalize(&mut self) {
        self.axioms.sort_by_key(|n| n.text_sym);
        self.resources.sort_by_key(|n| n.text_sym);
        // Goals and Decisions have positional IDs, so sorting them requires re-indexing.
        // For v0, we assume the encoder adds them in a stable order.
    }
}
