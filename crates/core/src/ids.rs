//! Stable identifiers for the session tree.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            pub const fn raw(self) -> u64 {
                self.0
            }

            /// Short label used in pane titles and debug output.
            pub fn label(self) -> String {
                format!("{}{}", $prefix, self.0)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.label())
            }
        }
    };
}

id_type!(WorkspaceId, "w");
id_type!(TabId, "t");
id_type!(PaneId, "p");

/// Allocates ids for one session. Ids are never reused, so a stale client
/// reference can never address a different pane.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ids {
    next_workspace: u64,
    next_tab: u64,
    next_pane: u64,
}

impl Ids {
    pub fn new() -> Self {
        Self {
            next_workspace: 1,
            next_tab: 1,
            next_pane: 1,
        }
    }

    pub fn workspace(&mut self) -> WorkspaceId {
        let id = WorkspaceId(self.next_workspace);
        self.next_workspace += 1;
        id
    }

    pub fn tab(&mut self) -> TabId {
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        id
    }

    pub fn pane(&mut self) -> PaneId {
        let id = PaneId(self.next_pane);
        self.next_pane += 1;
        id
    }

    /// Rebuilds the allocator so restored ids are never handed out twice.
    pub fn observe(&mut self, workspace: WorkspaceId, tab: TabId, pane: PaneId) {
        self.next_workspace = self.next_workspace.max(workspace.0 + 1);
        self.next_tab = self.next_tab.max(tab.0 + 1);
        self.next_pane = self.next_pane.max(pane.0 + 1);
    }

    pub fn next_pane_id(&self) -> PaneId {
        PaneId(self.next_pane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_observed() {
        let mut ids = Ids::default();
        // Default starts at zero; ids() would hand out 0. Callers use new().
        let mut ids = {
            let _ = &mut ids;
            Ids::new()
        };
        let a = ids.pane();
        let b = ids.pane();
        assert_ne!(a, b);
        ids.observe(WorkspaceId(7), TabId(3), PaneId(9));
        assert_eq!(ids.pane(), PaneId(10));
        assert_eq!(ids.tab(), TabId(4));
        assert_eq!(ids.workspace(), WorkspaceId(8));
    }
}
