use super::*;

fn sorted(names: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    v.sort_by(|a, b| natural_cmp(a, b));
    v
}

#[test]
fn numbers_sort_before_letters() {
    assert_eq!(
        sorted(&["apple", "1file", "banana"]),
        ["1file", "apple", "banana"]
    );
}

#[test]
fn numeric_runs_compare_as_numbers() {
    assert_eq!(
        sorted(&["file10", "file2", "file1"]),
        ["file1", "file2", "file10"]
    );
}

#[test]
fn comparison_is_case_insensitive() {
    assert_eq!(
        sorted(&["Zeta", "alpha", "Beta"]),
        ["alpha", "Beta", "Zeta"]
    );
}

#[test]
fn folders_sort_before_files() {
    let dir = |name: &str, is_dir: bool| TreeNode {
        name: name.to_string(),
        path: PathBuf::from(name),
        is_dir,
        icon: NodeIcon::Generic,
        loaded: false,
        children: Vec::new(),
    };
    let mut nodes = [
        dir("main.cpp", false),
        dir("src", true),
        dir("README.md", false),
        dir("include", true),
    ];
    nodes.sort_by(cmp_nodes);
    let order: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(order, ["include", "src", "main.cpp", "README.md"]);
}

#[test]
fn icon_resolution_covers_specials_and_fallback() {
    assert_eq!(icon_for_file("Makefile"), NodeIcon::Build);
    assert_eq!(icon_for_file(".gitignore"), NodeIcon::Git);
    assert_eq!(icon_for_file("effect.cpp"), NodeIcon::Cpp);
    assert_eq!(icon_for_file("effect.h"), NodeIcon::Header);
    assert_eq!(icon_for_file("notes.md"), NodeIcon::Markdown);
    assert_eq!(icon_for_file("mystery.xyz"), NodeIcon::Generic);
    assert_eq!(icon_for_file("README"), NodeIcon::Generic);
}

#[test]
fn refresh_dir_preserves_loaded_subtree_and_adds_new_entries() {
    let root = PathBuf::from("/proj");
    let mut listing = HashMap::new();
    listing.insert(
        root.clone(),
        vec![("src".into(), true), ("b.txt".into(), false)],
    );
    let fs = FakeFs {
        listing,
        ..Default::default()
    };

    let mut src = TreeNode {
        name: "src".into(),
        path: root.join("src"),
        is_dir: true,
        icon: NodeIcon::FolderClosed,
        loaded: true,
        children: vec![TreeNode {
            name: "main.cpp".into(),
            path: root.join("src/main.cpp"),
            is_dir: false,
            icon: NodeIcon::Cpp,
            loaded: false,
            children: Vec::new(),
        }],
    };
    src.loaded = true;
    let mut root_node = TreeNode {
        name: "proj".into(),
        path: root.clone(),
        is_dir: true,
        icon: NodeIcon::FolderClosed,
        loaded: true,
        children: vec![src],
    };

    refresh_dir(&mut root_node, &fs);

    let names: Vec<&str> = root_node.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["src", "b.txt"]);
    let src = &root_node.children[0];
    assert!(src.loaded);
    assert_eq!(src.children.len(), 1);
    assert_eq!(src.children[0].name, "main.cpp");
}

fn node(path: &str, is_dir: bool, icon: NodeIcon, children: Vec<TreeNode>) -> TreeNode {
    let path = PathBuf::from(path);
    TreeNode {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        is_dir,
        icon,
        loaded: true,
        children,
    }
}

#[test]
fn new_entries_go_into_the_selected_folder_or_beside_the_selected_file() {
    let main = node("/proj/src/main.cpp", false, NodeIcon::Cpp, Vec::new());
    let src = node("/proj/src", true, NodeIcon::FolderClosed, vec![main]);
    let mut tree = FileTreeState {
        root: node("/proj", true, NodeIcon::FolderClosed, vec![src]),
        selected: None,
    };

    assert_eq!(create_target(&tree), PathBuf::from("/proj"));

    tree.selected = Some(PathBuf::from("/proj/src"));
    assert_eq!(create_target(&tree), PathBuf::from("/proj/src"));

    tree.selected = Some(PathBuf::from("/proj/src/main.cpp"));
    assert_eq!(create_target(&tree), PathBuf::from("/proj/src"));

    tree.selected = Some(PathBuf::from("/proj/gone.txt"));
    assert_eq!(
        create_target(&tree),
        PathBuf::from("/proj"),
        "a stale selection falls back to the project root"
    );
}

#[test]
fn folders_show_open_or_closed_and_files_keep_their_own_icon() {
    let file = node("/proj/main.cpp", false, NodeIcon::Cpp, Vec::new());
    assert_eq!(row_icon(&file, None), NodeIcon::Cpp);
    assert_eq!(row_icon(&file, Some(true)), NodeIcon::FolderOpen);
    assert_eq!(row_icon(&file, Some(false)), NodeIcon::FolderClosed);

    assert_eq!(entry_kind_icon(EntryKind::File), NodeIcon::Generic);
    assert_eq!(
        entry_kind_icon(EntryKind::Directory),
        NodeIcon::FolderClosed
    );
}
