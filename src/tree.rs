//! ファイルツリーのデータ構造と操作。
//!
//! ## パスの規約
//! - `FileNode.name`: ファイル名（String、UTF-8前提）
//! - ツリー内のパス: `/` 区切りの相対パス（String）
//! - システムパス操作: `std::path::Path` / `PathBuf` を使用

use std::collections::BTreeMap;
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
/// - 両方シンボリックリンク → ターゲットパスを直接比較
/// - 片方だけシンボリックリンク → `Modified`（種別が異なる）
/// - size が異なる → `Modified`（確定）
/// - size 同じ + mtime 同じ → `Equal`
/// - size 同じ + mtime 異なる/不明 → `Undetermined`（コンテンツ比較が必要）
/// - size 不明 → `Undetermined`
pub fn compare_metadata(left: &FileNode, right: &FileNode) -> MetadataCmp {
    // 両方 symlink → ターゲットパスで比較
    if let (NodeKind::Symlink { target: lt }, NodeKind::Symlink { target: rt }) =
        (&left.kind, &right.kind)
    {
        return if lt == rt {
            MetadataCmp::Equal
        } else {
            MetadataCmp::Modified
        };
    }
    // 片方だけ symlink → 種別が異なるので Modified
    if left.is_symlink() || right.is_symlink() {
        return MetadataCmp::Modified;
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
/// `Some({})` は空ディレクトリを表す。
/// BTreeMap により find_node が O(log N) でルックアップでき、
/// イテレーション時はキー（名前）の昇順が保証される。
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
    /// 子ノード。None = 未取得（遅延読み込み）、Some({}) = 空ディレクトリ
    /// キー = ファイル名、値 = FileNode
    pub children: Option<BTreeMap<String, FileNode>>,
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
        let children_map: BTreeMap<String, FileNode> =
            children.into_iter().map(|n| (n.name.clone(), n)).collect();
        Self {
            name: name.into(),
            kind: NodeKind::Directory,
            size: None,
            mtime: None,
            permissions: None,
            children: Some(children_map),
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

    /// 子ノードを名前でソート（BTreeMap 化により no-op: キー順序は自動保証される）
    #[allow(clippy::unused_self)]
    pub fn sort_children(&mut self) {
        // BTreeMap はイテレーション時にキー昇順を自動保証するため、明示的なソートは不要。
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

    /// 指定パスまでの全ディレクトリノードが存在しなければ作成する。
    /// パスの全コンポーネントをディレクトリとして確保する（最後の要素も含む）。
    /// 戻り値: 新規作成したノード数
    pub fn ensure_path(&mut self, path: &Path) -> usize {
        let components: Vec<String> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
            .collect();
        if components.is_empty() {
            return 0;
        }
        let mut created = 0;
        ensure_path_in_nodes(&mut self.nodes, &components, &mut created);
        created
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
/// children は BTreeMap のためソート不要（名前昇順は自動）。nodes スライスのみソートする。
fn sort_nodes(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        let a_is_dir = a.is_dir() as u8;
        let b_is_dir = b.is_dir() as u8;
        b_is_dir.cmp(&a_is_dir).then(a.name.cmp(&b.name))
    });
    // children は BTreeMap のため再帰ソート不要
}

/// `ensure_path` の再帰ヘルパー。
/// `nodes` (Vec) を辿り、パスコンポーネントに対応するディレクトリを作成する。
fn ensure_path_in_nodes(nodes: &mut Vec<FileNode>, components: &[String], created: &mut usize) {
    if components.is_empty() {
        return;
    }
    let part = &components[0];
    let idx = nodes.iter().position(|n| n.name == *part);
    let idx = match idx {
        Some(i) => i,
        None => {
            nodes.push(FileNode::new_dir(part));
            *created += 1;
            nodes.len() - 1
        }
    };
    let node = &mut nodes[idx];
    if node.children.is_none() {
        node.children = Some(BTreeMap::new());
    }
    if components.len() > 1 {
        // children は BTreeMap<String, FileNode> なのでそのままは Vec として渡せない。
        // children を一時的に取り出して処理し、再セットする。
        let mut children_map = node.children.take().unwrap();
        let child_part = &components[1];
        if !children_map.contains_key(child_part.as_str()) {
            children_map.insert(child_part.clone(), FileNode::new_dir(child_part));
            *created += 1;
        }
        // さらに深い階層は children_map のエントリを再帰的に処理
        if components.len() > 2 {
            let child_node = children_map.get_mut(child_part.as_str()).unwrap();
            if child_node.children.is_none() {
                child_node.children = Some(BTreeMap::new());
            }
            ensure_path_in_btree_node(child_node, &components[2..], created);
        }
        node.children = Some(children_map);
    }
}

/// BTreeMap の children を辿ってパスを確保する再帰ヘルパー。
fn ensure_path_in_btree_node(node: &mut FileNode, components: &[String], created: &mut usize) {
    if components.is_empty() {
        return;
    }
    let part = &components[0];
    let children = node.children.get_or_insert_with(BTreeMap::new);
    if !children.contains_key(part.as_str()) {
        children.insert(part.clone(), FileNode::new_dir(part));
        *created += 1;
    }
    if components.len() > 1 {
        let child = children.get_mut(part.as_str()).unwrap();
        if child.children.is_none() {
            child.children = Some(BTreeMap::new());
        }
        ensure_path_in_btree_node(child, &components[1..], created);
    }
}

/// パスコンポーネント列を辿ってノードを可変参照で検索する（O(log N) ルックアップ）。
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
        // BTreeMap の O(log N) ルックアップで再帰
        let next = children.get_mut(path[1])?;
        if path.len() == 2 {
            Some(next)
        } else {
            find_node_mut_in_btree(next, &path[2..])
        }
    } else {
        None
    }
}

/// BTreeMap の children を辿ってノードを可変参照で検索する。
fn find_node_mut_in_btree<'a>(node: &'a mut FileNode, path: &[&str]) -> Option<&'a mut FileNode> {
    if path.is_empty() {
        return Some(node);
    }
    let children = node.children.as_mut()?;
    let next = children.get_mut(path[0])?;
    find_node_mut_in_btree(next, &path[1..])
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
        // BTreeMap の O(log N) ルックアップで次のコンポーネントを探す
        match children.get(path[1]) {
            None => NodePresence::NotFound,
            Some(next) => {
                if path.len() == 2 {
                    NodePresence::Found
                } else {
                    find_node_presence_in_btree(next, &path[2..])
                }
            }
        }
    } else {
        // children が None = 未ロードディレクトリ → 子の存在は不明
        NodePresence::Unloaded
    }
}

/// BTreeMap の children を辿ってノードの存在を3値で判定する。
fn find_node_presence_in_btree(node: &FileNode, path: &[&str]) -> NodePresence {
    if path.is_empty() {
        return NodePresence::Found;
    }
    match &node.children {
        None => NodePresence::Unloaded,
        Some(children) => match children.get(path[0]) {
            None => NodePresence::NotFound,
            Some(next) => find_node_presence_in_btree(next, &path[1..]),
        },
    }
}

/// パスコンポーネント列を辿ってノードを不変参照で検索する（O(log N) ルックアップ）。
fn find_node_recursive<'a>(nodes: &'a [FileNode], path: &[&str]) -> Option<&'a FileNode> {
    if path.is_empty() {
        return None;
    }

    let name = path[0];
    let node = nodes.iter().find(|n| n.name == name)?;

    if path.len() == 1 {
        Some(node)
    } else if let Some(ref children) = node.children {
        // BTreeMap の O(log N) ルックアップで再帰
        let next = children.get(path[1])?;
        if path.len() == 2 {
            Some(next)
        } else {
            find_node_in_btree(next, &path[2..])
        }
    } else {
        None
    }
}

/// BTreeMap の children を辿ってノードを検索する（O(log N) ルックアップ）。
fn find_node_in_btree<'a>(node: &'a FileNode, path: &[&str]) -> Option<&'a FileNode> {
    if path.is_empty() {
        return Some(node);
    }
    let children = node.children.as_ref()?;
    let next = children.get(path[0])?;
    find_node_in_btree(next, &path[1..])
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
            .values()
            .map(|n| n.name.as_str())
            .collect();
        // BTreeMap はキー昇順（名前昇順）なので sort_children は no-op
        assert_eq!(names, vec!["alpha", "beta.txt", "gamma", "zebra.txt"]);
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
        node.children = Some(
            vec![FileNode::new_file("helper.rs")]
                .into_iter()
                .map(|n| (n.name.clone(), n))
                .collect(),
        );
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
    fn test_compare_metadata_symlink_same_target() {
        // 同一ターゲットの symlink → Equal
        let l = FileNode::new_symlink("link", "/usr/share/target");
        let r = FileNode::new_symlink("link", "/usr/share/target");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Equal);
    }

    #[test]
    fn test_compare_metadata_symlink_different_target() {
        // 異なるターゲットの symlink → Modified
        let l = FileNode::new_symlink("link", "/usr/share/old");
        let r = FileNode::new_symlink("link", "/usr/share/new");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Modified);
    }

    #[test]
    fn test_compare_metadata_symlink_vs_file() {
        // 片方だけ symlink → Modified
        let l = FileNode::new_symlink("link", "target");
        let r = FileNode::new_file("link");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Modified);
    }

    #[test]
    fn test_compare_metadata_file_vs_symlink() {
        // 片方だけ symlink（逆方向）→ Modified
        let l = FileNode::new_file("link");
        let r = FileNode::new_symlink("link", "target");
        assert_eq!(compare_metadata(&l, &r), MetadataCmp::Modified);
    }

    // ── ensure_path ──

    #[test]
    fn test_ensure_path_all_exist() {
        // 全コンポーネントが既に存在する場合 → 何も作成しない
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "a",
                vec![FileNode::new_dir_with_children(
                    "b",
                    vec![FileNode::new_dir_with_children("c", vec![])],
                )],
            )],
        };
        let created = tree.ensure_path(Path::new("a/b/c"));
        assert_eq!(created, 0);
    }

    #[test]
    fn test_ensure_path_one_missing() {
        // 中間ディレクトリが1つ不在 → 作成して1返却
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "a",
                vec![FileNode::new_dir_with_children("b", vec![])],
            )],
        };
        let created = tree.ensure_path(Path::new("a/b/c"));
        assert_eq!(created, 1);
        assert!(tree.find_node(Path::new("a/b/c")).is_some());
    }

    #[test]
    fn test_ensure_path_multiple_missing() {
        // 複数レベル不在（a/b/c/d で a のみ存在）→ b, c, d を作成
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir("a")],
        };
        // a は未ロード（children: None）
        let created = tree.ensure_path(Path::new("a/b/c/d"));
        assert_eq!(created, 3); // b, c, d を作成
        assert!(tree.find_node(Path::new("a/b/c/d")).is_some());
    }

    #[test]
    fn test_ensure_path_empty() {
        // 空パス → 0返却
        let mut tree = FileTree::new("/test");
        let created = tree.ensure_path(Path::new(""));
        assert_eq!(created, 0);
    }

    #[test]
    fn test_ensure_path_unloaded_gets_initialized() {
        // 既存ノードが未ロード（children: None）→ Some(Vec::new()) に変更される
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir("a")], // children: None
        };
        assert!(!tree.find_node(Path::new("a")).unwrap().is_loaded());

        tree.ensure_path(Path::new("a"));
        // children が Some に変わる
        assert!(tree.find_node(Path::new("a")).unwrap().is_loaded());
    }

    #[test]
    fn test_ensure_path_loaded_preserves_children() {
        // 既存ノードがロード済み（children: Some([...])）→ 既存の children は保持される
        let mut tree = FileTree {
            root: PathBuf::from("/test"),
            nodes: vec![FileNode::new_dir_with_children(
                "a",
                vec![FileNode::new_file("existing.txt")],
            )],
        };

        let created = tree.ensure_path(Path::new("a/b"));
        assert_eq!(created, 1);

        // 既存の existing.txt は保持される
        let a_node = tree.find_node(Path::new("a")).unwrap();
        let children = a_node.children.as_ref().unwrap();
        assert!(children.contains_key("existing.txt"));
        assert!(children.contains_key("b"));
    }
}
