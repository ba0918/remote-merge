//! ファイルツリーのデータ構造と操作。
//!
//! ## パスの規約
//! - `FileNode.name`: ファイル名（String、UTF-8前提）
//! - ツリー内のパス: `/` 区切りの相対パス（String）
//! - システムパス操作: `std::path::Path` / `PathBuf` を使用

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

/// メタデータ比較の結果。
///
/// CLI status / TUI badge 共通で使う差分判定の基盤。
/// `Undetermined` はコンテンツ比較が必要であることを示す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataCmp {
    /// メタデータ上同一（size + mtime 一致）
    Equal,
    /// メタデータ上確実に異なる（size 不一致）
    Modified,
    /// メタデータだけでは判定不能（size 一致 + mtime 不一致/不明、シンボリックリンク等）
    Undetermined,
}

/// 2つのファイルノードのメタデータを比較する（純粋関数）。
///
/// 判定ルール:
/// - シンボリックリンク → `Undetermined`（ターゲット文字列比較が必要）
/// - size が異なる → `Modified`（確定）
/// - size 同じ + mtime 同じ → `Equal`
/// - size 同じ + mtime 異なる/不明 → `Undetermined`（コンテンツ比較が必要）
/// - size 不明 → `Undetermined`
pub fn compare_metadata(left: &FileNode, right: &FileNode) -> MetadataCmp {
    // シンボリックリンクはメタデータ比較できない
    if left.is_symlink() || right.is_symlink() {
        return MetadataCmp::Undetermined;
    }

    match (left.size, right.size) {
        (Some(ls), Some(rs)) if ls != rs => MetadataCmp::Modified,
        (Some(_), Some(_)) => match (left.mtime, right.mtime) {
            (Some(lt), Some(rt)) if lt == rt => MetadataCmp::Equal,
            _ => MetadataCmp::Undetermined,
        },
        _ => MetadataCmp::Undetermined,
    }
}

/// ファイルノードの種別
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// 通常ファイル
    File,
    /// ディレクトリ
    Directory,
    /// シンボリックリンク（リンク先パスを保持）
    Symlink { target: String },
}

/// ファイルツリーの1ノード
///
/// `children` が `None` の場合は未取得（遅延読み込み）を表す。
/// `Some(vec![])` は空ディレクトリを表す。
#[derive(Debug, Clone)]
pub struct FileNode {
    /// ファイル/ディレクトリ名
    pub name: String,
    /// ノード種別
    pub kind: NodeKind,
    /// ファイルサイズ（バイト）
    pub size: Option<u64>,
    /// 最終更新日時
    pub mtime: Option<DateTime<Utc>>,
    /// Unix パーミッション (例: 0o644)
    pub permissions: Option<u32>,
    /// 子ノード。None = 未取得（遅延読み込み）、Some([]) = 空ディレクトリ
    pub children: Option<Vec<FileNode>>,
}

impl FileNode {
    /// 新しいファイルノードを作成
    pub fn new_file(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::File,
            size: None,
            mtime: None,
            permissions: None,
            children: None,
        }
    }

    /// 新しいディレクトリノードを作成（未取得状態）
    pub fn new_dir(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Directory,
            size: None,
            mtime: None,
            permissions: None,
            children: None, // 未取得
        }
    }

    /// 新しいディレクトリノードを子ノード付きで作成
    pub fn new_dir_with_children(name: impl Into<String>, children: Vec<FileNode>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Directory,
            size: None,
            mtime: None,
            permissions: None,
            children: Some(children),
        }
    }

    /// 新しいシンボリックリンクノードを作成
    pub fn new_symlink(name: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Symlink {
                target: target.into(),
            },
            size: None,
            mtime: None,
            permissions: None,
            children: None,
        }
    }

    /// ディレクトリかどうか
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, NodeKind::Directory)
    }

    /// ファイルかどうか
    pub fn is_file(&self) -> bool {
        matches!(self.kind, NodeKind::File)
    }

    /// シンボリックリンクかどうか
    pub fn is_symlink(&self) -> bool {
        matches!(self.kind, NodeKind::Symlink { .. })
    }

    /// 子ノードが取得済みかどうか
    pub fn is_loaded(&self) -> bool {
        self.children.is_some()
    }

    /// 子ノードを名前でソート
    pub fn sort_children(&mut self) {
        if let Some(ref mut children) = self.children {
            children.sort_by(|a, b| {
                // ディレクトリを先に、その後名前順
                let a_is_dir = a.is_dir() as u8;
                let b_is_dir = b.is_dir() as u8;
                b_is_dir.cmp(&a_is_dir).then(a.name.cmp(&b.name))
            });
        }
    }
}

/// ファイルツリー全体を表すルートコンテナ
#[derive(Debug, Clone, Default)]
pub struct FileTree {
    /// ツリーのルートパス
    pub root: PathBuf,
    /// ルート配下のノード
    pub nodes: Vec<FileNode>,
}

impl FileTree {
    /// 指定ルートパスで空のファイルツリーを作成する。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            nodes: Vec::new(),
        }
    }

    /// ノードを名前でソート
    pub fn sort(&mut self) {
        sort_nodes(&mut self.nodes);
    }

    /// 指定パスのノードを検索（相対パス）。
    ///
    /// `rel_path` には `&str`、`&Path`、`&PathBuf` など `AsRef<Path>` を実装する
    /// 任意の型を渡すことができる。パスは `/` 区切りの相対パスとして解釈される。
    pub fn find_node(&self, rel_path: impl AsRef<Path>) -> Option<&FileNode> {
        let rel_path = rel_path.as_ref();
        let components: Vec<&str> = rel_path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""))
            .collect();
        find_node_recursive(&self.nodes, &components)
    }

    /// 指定パスのノードを検索し、途中で未ロードかどうかも区別する。
    ///
    /// `find_node` と異なり、途中のディレクトリが未ロード（`children: None`）の場合に
    /// `NodePresence::Unloaded` を返す。これにより「存在しない」と「判定不能」を区別できる。
    pub fn find_node_or_unloaded(&self, rel_path: impl AsRef<Path>) -> NodePresence {
        let rel_path = rel_path.as_ref();
        let components: Vec<&str> = rel_path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""))
            .collect();
        find_node_presence_recursive(&self.nodes, &components)
    }

    /// 指定パスのノードを可変参照で検索（相対パス）。
    ///
    /// `rel_path` には `&str`、`&Path`、`&PathBuf` など `AsRef<Path>` を実装する
    /// 任意の型を渡すことができる。パスは `/` 区切りの相対パスとして解釈される。
    pub fn find_node_mut(&mut self, rel_path: impl AsRef<Path>) -> Option<&mut FileNode> {
        let rel_path = rel_path.as_ref();
        let components: Vec<&str> = rel_path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""))
            .collect();
        find_node_mut_recursive(&mut self.nodes, &components)
    }
}

/// ノードリストを再帰的にソートする（ディレクトリ優先、名前順）。
fn sort_nodes(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        let a_is_dir = a.is_dir() as u8;
        let b_is_dir = b.is_dir() as u8;
        b_is_dir.cmp(&a_is_dir).then(a.name.cmp(&b.name))
    });
    for node in nodes.iter_mut() {
        if let Some(ref mut children) = node.children {
            sort_nodes(children);
        }
    }
}

/// パスコンポーネント列を辿ってノードを可変参照で検索する。
fn find_node_mut_recursive<'a>(
    nodes: &'a mut [FileNode],
    path: &[&str],
) -> Option<&'a mut FileNode> {
    if path.is_empty() {
        return None;
    }

    let name = path[0];
    let node = nodes.iter_mut().find(|n| n.name == name)?;

    if path.len() == 1 {
        Some(node)
    } else if let Some(ref mut children) = node.children {
        find_node_mut_recursive(children, &path[1..])
    } else {
        None
    }
}

/// ノード検索結果（見つかった / 途中が未ロード / 存在しない）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePresence {
    /// ノードが見つかった
    Found,
    /// 途中のディレクトリが未ロードのため判定不能
    Unloaded,
    /// ツリー上に存在しない
    NotFound,
}

/// ノードスライスから `/` 区切りのパス文字列でノードを検索する。
///
/// `FileTree::find_node` と同じロジックだが、`&[FileNode]` に直接使える。
pub fn find_node_in_slice<'a>(nodes: &'a [FileNode], path: &str) -> Option<&'a FileNode> {
    let parts: Vec<&str> = path.split('/').collect();
    find_node_recursive(nodes, &parts)
}

/// パスコンポーネント列を辿ってノードの存在を3値で判定する。
fn find_node_presence_recursive(nodes: &[FileNode], path: &[&str]) -> NodePresence {
    if path.is_empty() {
        return NodePresence::NotFound;
    }

    let name = path[0];
    let node = match nodes.iter().find(|n| n.name == name) {
        Some(n) => n,
        None => return NodePresence::NotFound,
    };

    if path.len() == 1 {
        NodePresence::Found
    } else if let Some(ref children) = node.children {
        find_node_presence_recursive(children, &path[1..])
    } else {
        // children が None = 未ロードディレクトリ → 子の存在は不明
        NodePresence::Unloaded
    }
}

/// パスコンポーネント列を辿ってノードを不変参照で検索する。
fn find_node_recursive<'a>(nodes: &'a [FileNode], path: &[&str]) -> Option<&'a FileNode> {
    if path.is_empty() {
        return None;
    }

    let name = path[0];
    let node = nodes.iter().find(|n| n.name == name)?;

    if path.len() == 1 {
        Some(node)
    } else if let Some(ref children) = node.children {
        find_node_recursive(children, &path[1..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_node_creation() {
        let file = FileNode::new_file("test.txt");
        assert!(file.is_file());
        assert!(!file.is_dir());
        assert!(!file.is_symlink());
        assert!(!file.is_loaded());
    }

    #[test]
    fn test_dir_node_unloaded() {
        let dir = FileNode::new_dir("src");
        assert!(dir.is_dir());
        assert!(!dir.is_loaded()); // children: None = 未取得
    }

    #[test]
    fn test_dir_node_with_children() {
        let dir = FileNode::new_dir_with_children(
            "src",
            vec![FileNode::new_file("main.rs"), FileNode::new_file("lib.rs")],
        );
        assert!(dir.is_dir());
        assert!(dir.is_loaded());
        assert_eq!(dir.children.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_empty_dir() {
        let dir = FileNode::new_dir_with_children("empty", vec![]);
        assert!(dir.is_dir());
        assert!(dir.is_loaded());
        assert_eq!(dir.children.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_symlink_node() {
        let link = FileNode::new_symlink("config.json", "../shared/config.json");
        assert!(link.is_symlink());
        assert!(!link.is_file());
        if let NodeKind::Symlink { ref target } = link.kind {
            assert_eq!(target, "../shared/config.json");
        } else {
            panic!("Expected Symlink");
        }
    }

    #[test]
    fn test_sort_children() {
        let mut dir = FileNode::new_dir_with_children(
            "root",
            vec![
                FileNode::new_file("zebra.txt"),
                FileNode::new_dir("alpha"),
                FileNode::new_file("beta.txt"),
                FileNode::new_dir("gamma"),
            ],
        );
        dir.sort_children();
        let names: Vec<&str> = dir
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        // ディレクトリが先、その後名前順
        assert_eq!(names, vec!["alpha", "gamma", "beta.txt", "zebra.txt"]);
    }

    #[test]
    fn test_file_tree_find_node() {
        let tree = FileTree {
            root: PathBuf::from("/home/user/app"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![
                    FileNode::new_file("main.rs"),
                    FileNode::new_dir_with_children("utils", vec![FileNode::new_file("helper.rs")]),
                ],
            )],
        };

        assert!(tree.find_node(Path::new("src")).is_some());
        assert!(tree.find_node(Path::new("src/main.rs")).is_some());
        assert!(tree.find_node(Path::new("src/utils/helper.rs")).is_some());
        assert!(tree.find_node(Path::new("nonexistent")).is_none());
    }

    #[test]
    fn test_find_node_mut() {
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs"), FileNode::new_dir("utils")],
            )],
        };

        // ノードを可変参照で取得して変更
        let node = tree.find_node_mut(Path::new("src/utils")).unwrap();
        assert!(node.is_dir());
        assert!(!node.is_loaded());

        // children を設定
        node.children = Some(vec![FileNode::new_file("helper.rs")]);
        assert!(node.is_loaded());

        // 変更が反映されているか確認
        let node = tree.find_node(Path::new("src/utils")).unwrap();
        assert!(node.is_loaded());
        assert_eq!(node.children.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_find_node_mut_nonexistent() {
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_file("a.txt")],
        };
        assert!(tree.find_node_mut(Path::new("nonexistent")).is_none());
    }

    #[test]
    fn test_find_node_or_unloaded_found() {
        let tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_file("main.rs")],
            )],
        };
        assert_eq!(
            tree.find_node_or_unloaded(Path::new("src/main.rs")),
            NodePresence::Found
        );
    }

    #[test]
    fn test_find_node_or_unloaded_not_found() {
        let tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children("src", vec![])],
        };
        // src はロード済み（空）→ main.rs は確実に存在しない
        assert_eq!(
            tree.find_node_or_unloaded(Path::new("src/main.rs")),
            NodePresence::NotFound
        );
    }

    #[test]
    fn test_find_node_or_unloaded_unloaded_parent() {
        let tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir("src")], // children: None = 未ロード
        };
        // src が未ロードなので子の存在は判定不能
        assert_eq!(
            tree.find_node_or_unloaded(Path::new("src/main.rs")),
            NodePresence::Unloaded
        );
    }

    #[test]
    fn test_find_node_or_unloaded_deep_unloaded() {
        let tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "src",
                vec![FileNode::new_dir("app")], // app は未ロード
            )],
        };
        // src/app が未ロードなので src/app/mod.rs の存在は判定不能
        assert_eq!(
            tree.find_node_or_unloaded(Path::new("src/app/mod.rs")),
            NodePresence::Unloaded
        );
    }

    // ── compare_metadata ──

    fn make_file_with_meta(name: &str, size: u64, mtime: Option<DateTime<Utc>>) -> FileNode {
        let mut node = FileNode::new_file(name);
        node.size = Some(size);
        node.mtime = mtime;
        node
    }

    #[test]
    fn test_compare_metadata_equal() {
        use chrono::TimeZone;
        let ts = Utc.timestamp_opt(1700000000, 0).unwrap();
        let l = make_file_with_meta("a", 100, Some(ts));
        let r = make_file_with_meta("a", 100, Some(ts));
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Equal);
    }

    #[test]
    fn test_compare_metadata_different_size() {
        use chrono::TimeZone;
        let ts = Utc.timestamp_opt(1700000000, 0).unwrap();
        let l = make_file_with_meta("a", 100, Some(ts));
        let r = make_file_with_meta("a", 200, Some(ts));
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Modified);
    }

    #[test]
    fn test_compare_metadata_same_size_different_mtime() {
        use chrono::TimeZone;
        let ts1 = Utc.timestamp_opt(1700000000, 0).unwrap();
        let ts2 = Utc.timestamp_opt(1700000001, 0).unwrap();
        let l = make_file_with_meta("a", 100, Some(ts1));
        let r = make_file_with_meta("a", 100, Some(ts2));
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Undetermined);
    }

    #[test]
    fn test_compare_metadata_no_size() {
        let l = FileNode::new_file("a");
        let r = FileNode::new_file("a");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Undetermined);
    }

    #[test]
    fn test_compare_metadata_symlink() {
        let l = FileNode::new_symlink("link", "target");
        let r = FileNode::new_symlink("link", "target");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Undetermined);
    }
}
