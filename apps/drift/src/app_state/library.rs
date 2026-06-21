use super::{App, Action, Symbol};

/// A folder in the library panel for organizing symbols.
#[derive(Clone, Debug)]
pub struct LibraryFolder {
    pub id: usize,
    pub name: String,
    pub parent_id: Option<usize>,
}

impl App {
    pub fn apply_library(&mut self, action: Action) {
        match action {
            Action::CreateLibraryFolder { name, parent_id } => {
                let id = self.next_folder_id;
                self.next_folder_id += 1;
                self.library_folders.push(LibraryFolder { id, name, parent_id });
            }
            Action::RenameLibraryFolder { folder_id, name } => {
                if let Some(f) = self.library_folders.iter_mut().find(|f| f.id == folder_id) {
                    f.name = name;
                }
            }
            Action::DeleteLibraryFolder { folder_id } => {
                self.library_folders.retain(|f| f.id != folder_id);
                // Move symbols in this folder to root (None).
                for (_, fid) in self.symbol_folder_map.iter_mut() {
                    if *fid == Some(folder_id) {
                        *fid = None;
                    }
                }
            }
            Action::MoveSymbolToFolder { symbol_id, folder_id } => {
                // Validate folder exists (or folder_id is None = root).
                if let Some(fid) = folder_id {
                    if !self.library_folders.iter().any(|f| f.id == fid) {
                        return;
                    }
                }
                if let Some(entry) = self.symbol_folder_map.iter_mut().find(|(sid, _)| *sid == symbol_id) {
                    entry.1 = folder_id;
                } else {
                    self.symbol_folder_map.push((symbol_id, folder_id));
                }
            }
            Action::SetLibrarySearch { query } => {
                self.library_search = query;
            }
            _ => {}
        }
    }

    /// Returns all symbols in a given folder (None = root) filtered by the current search query.
    pub fn library_symbols_in_folder(&self, folder_id: Option<usize>) -> Vec<&Symbol> {
        let query = self.library_search.to_lowercase();
        self.symbols
            .iter()
            .filter(|sym| {
                // Check folder membership.
                let in_folder = match self.symbol_folder_map.iter().find(|(sid, _)| *sid == sym.id) {
                    Some((_, fid)) => *fid == folder_id,
                    None => folder_id.is_none(), // unmapped symbols are in root
                };
                if !in_folder {
                    return false;
                }
                // Apply search filter.
                if query.is_empty() {
                    true
                } else {
                    sym.name.to_lowercase().contains(&query)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::SymbolKind;

    fn app() -> App {
        App::new()
    }

    fn mk_symbol(a: &mut App, name: &str) -> usize {
        a.apply(Action::CreateSymbol {
            name: name.to_string(),
            kind: SymbolKind::Graphic,
            width: 50.0,
            height: 50.0,
        });
        a.symbols.last().unwrap().id
    }

    #[test]
    fn test_create_library_folder() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Characters".to_string(), parent_id: None });
        assert_eq!(a.library_folders.len(), 1);
        assert_eq!(a.library_folders[0].name, "Characters");
    }

    #[test]
    fn test_create_nested_folder() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Assets".to_string(), parent_id: None });
        let parent_id = a.library_folders[0].id;
        a.apply(Action::CreateLibraryFolder { name: "Sub".to_string(), parent_id: Some(parent_id) });
        assert_eq!(a.library_folders[1].parent_id, Some(parent_id));
    }

    #[test]
    fn test_rename_library_folder() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Old".to_string(), parent_id: None });
        let fid = a.library_folders[0].id;
        a.apply(Action::RenameLibraryFolder { folder_id: fid, name: "New".to_string() });
        assert_eq!(a.library_folders[0].name, "New");
    }

    #[test]
    fn test_delete_library_folder() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Temp".to_string(), parent_id: None });
        let fid = a.library_folders[0].id;
        a.apply(Action::DeleteLibraryFolder { folder_id: fid });
        assert!(a.library_folders.is_empty());
    }

    #[test]
    fn test_delete_folder_moves_symbols_to_root() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Folder".to_string(), parent_id: None });
        let fid = a.library_folders[0].id;
        let sid = mk_symbol(&mut a, "Hero");
        a.apply(Action::MoveSymbolToFolder { symbol_id: sid, folder_id: Some(fid) });
        a.apply(Action::DeleteLibraryFolder { folder_id: fid });
        // Symbol should now be in root.
        let entry = a.symbol_folder_map.iter().find(|(s, _)| *s == sid);
        assert_eq!(entry.unwrap().1, None);
    }

    #[test]
    fn test_move_symbol_to_folder() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "Chars".to_string(), parent_id: None });
        let fid = a.library_folders[0].id;
        let sid = mk_symbol(&mut a, "Hero");
        a.apply(Action::MoveSymbolToFolder { symbol_id: sid, folder_id: Some(fid) });
        let entry = a.symbol_folder_map.iter().find(|(s, _)| *s == sid);
        assert_eq!(entry.unwrap().1, Some(fid));
    }

    #[test]
    fn test_move_symbol_to_nonexistent_folder_noop() {
        let mut a = app();
        let sid = mk_symbol(&mut a, "A");
        a.apply(Action::MoveSymbolToFolder { symbol_id: sid, folder_id: Some(9999) });
        // No mapping should be created.
        assert!(a.symbol_folder_map.is_empty());
    }

    #[test]
    fn test_set_library_search() {
        let mut a = app();
        a.apply(Action::SetLibrarySearch { query: "hero".to_string() });
        assert_eq!(a.library_search, "hero");
    }

    #[test]
    fn test_library_symbols_in_folder_root() {
        let mut a = app();
        mk_symbol(&mut a, "Tree");
        mk_symbol(&mut a, "Rock");
        let syms = a.library_symbols_in_folder(None);
        assert_eq!(syms.len(), 2);
    }

    #[test]
    fn test_library_symbols_in_folder_filtered() {
        let mut a = app();
        a.apply(Action::CreateLibraryFolder { name: "FX".to_string(), parent_id: None });
        let fid = a.library_folders[0].id;
        let sid = mk_symbol(&mut a, "Explosion");
        mk_symbol(&mut a, "Background");
        a.apply(Action::MoveSymbolToFolder { symbol_id: sid, folder_id: Some(fid) });
        let syms = a.library_symbols_in_folder(Some(fid));
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Explosion");
    }

    #[test]
    fn test_library_search_filters_symbols() {
        let mut a = app();
        mk_symbol(&mut a, "Hero");
        mk_symbol(&mut a, "Villain");
        a.apply(Action::SetLibrarySearch { query: "hero".to_string() });
        let syms = a.library_symbols_in_folder(None);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Hero");
    }

    #[test]
    fn test_library_search_case_insensitive() {
        let mut a = app();
        mk_symbol(&mut a, "HERO");
        a.apply(Action::SetLibrarySearch { query: "hero".to_string() });
        let syms = a.library_symbols_in_folder(None);
        assert_eq!(syms.len(), 1);
    }
}
