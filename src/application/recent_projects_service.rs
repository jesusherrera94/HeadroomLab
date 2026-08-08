//! Owns the in-memory recent-projects list and keeps it persisted through the
//! `RecentProjectsStore` port. Most-recent-first, deduped by path, capped at 5.

use std::rc::Rc;

use crate::application::ports::RecentProjectsStore;
use crate::domain::project::RecentProject;

/// Maximum number of recent projects kept.
const MAX_RECENTS: usize = 5;

pub struct RecentProjectsService {
    store: Rc<dyn RecentProjectsStore>,
    items: Vec<RecentProject>,
}

impl RecentProjectsService {
    /// Loads the persisted list on construction (empty if none/corrupt).
    pub fn new(store: Rc<dyn RecentProjectsStore>) -> Self {
        let mut items = store.load();
        Self::normalize(&mut items);
        Self { store, items }
    }

    /// The current list, most-recent-first.
    pub fn list(&self) -> &[RecentProject] {
        &self.items
    }

    /// Records a project: removes any existing entry for the same path, pushes
    /// this one to the front, truncates to the cap and persists. A save failure
    /// is logged; the in-memory list still reflects the change.
    pub fn record(&mut self, project: RecentProject) {
        self.items.retain(|p| !same_path(&p.path, &project.path));
        self.items.insert(0, project);
        self.items.truncate(MAX_RECENTS);

        if let Err(e) = self.store.save(&self.items) {
            eprintln!("[RecentProjectsService] {e}");
        }
    }

    /// Empties the list and persists that (File ▸ Open Recent ▸ Clear Menu).
    /// A save failure is logged, exactly as `record` treats one.
    pub fn clear(&mut self) {
        self.items.clear();
        if let Err(e) = self.store.save(&self.items) {
            eprintln!("[RecentProjectsService] {e}");
        }
    }

    /// Dedups (keeping the first/most-recent occurrence) and caps a freshly
    /// loaded list, tolerating a hand-edited or stale file.
    fn normalize(items: &mut Vec<RecentProject>) {
        let mut seen: Vec<std::path::PathBuf> = Vec::new();
        items.retain(|p| {
            if seen.iter().any(|s| same_path(s, &p.path)) {
                false
            } else {
                seen.push(p.path.clone());
                true
            }
        });
        items.truncate(MAX_RECENTS);
    }
}

/// Compares two paths, canonicalizing where the target exists on disk so the
/// same directory reached by different spellings dedups; falls back to a raw
/// comparison for not-yet-created paths (e.g. a Create target).
fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    use crate::application::ports::RecentProjectsError;

    /// In-memory store standing in for the file adapter.
    #[derive(Default)]
    struct FakeStore {
        saved: RefCell<Vec<RecentProject>>,
    }

    impl RecentProjectsStore for FakeStore {
        fn load(&self) -> Vec<RecentProject> {
            self.saved.borrow().clone()
        }
        fn save(&self, items: &[RecentProject]) -> Result<(), RecentProjectsError> {
            *self.saved.borrow_mut() = items.to_vec();
            Ok(())
        }
    }

    fn proj(name: &str, path: &str) -> RecentProject {
        RecentProject::new(name, PathBuf::from(path))
    }

    #[test]
    fn record_puts_newest_first() {
        let mut svc = RecentProjectsService::new(Rc::new(FakeStore::default()));
        svc.record(proj("a", "/a"));
        svc.record(proj("b", "/b"));
        let names: Vec<_> = svc.list().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
    }

    #[test]
    fn record_dedups_by_path_and_moves_to_top() {
        let mut svc = RecentProjectsService::new(Rc::new(FakeStore::default()));
        svc.record(proj("a", "/a"));
        svc.record(proj("b", "/b"));
        svc.record(proj("a-again", "/a"));
        assert_eq!(svc.list().len(), 2);
        assert_eq!(svc.list()[0].path, PathBuf::from("/a"));
        assert_eq!(svc.list()[0].name, "a-again");
    }

    #[test]
    fn record_caps_at_five() {
        let mut svc = RecentProjectsService::new(Rc::new(FakeStore::default()));
        for i in 0..8 {
            svc.record(proj(&format!("p{i}"), &format!("/p{i}")));
        }
        assert_eq!(svc.list().len(), 5);
        assert_eq!(svc.list()[0].name, "p7");
    }

    #[test]
    fn clear_empties_the_list_and_the_store() {
        let store = Rc::new(FakeStore::default());
        let mut svc = RecentProjectsService::new(store.clone());
        svc.record(proj("a", "/a"));
        svc.record(proj("b", "/b"));

        svc.clear();
        assert!(svc.list().is_empty());
        assert!(store.saved.borrow().is_empty());
        // And it stays cleared across a reload.
        assert!(RecentProjectsService::new(store).list().is_empty());
    }

    #[test]
    fn record_persists_through_store() {
        let store = Rc::new(FakeStore::default());
        let mut svc = RecentProjectsService::new(store.clone());
        svc.record(proj("a", "/a"));
        assert_eq!(store.saved.borrow().len(), 1);

        // A new service over the same store reloads what was saved.
        let reloaded = RecentProjectsService::new(store);
        assert_eq!(reloaded.list()[0].name, "a");
    }
}
