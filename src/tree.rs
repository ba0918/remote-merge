//! ファイルツリーのデータ構造と操作。
//!
//! ## パスの規約
//! - `FileNode.name`: ファイル名（String、UTF-8前提）
//! - ツリー内のパス: `/` 区切りの相対パス（String）
//! - システムパス操作: `std::path::Path` / `PathBuf` を使用

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

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
#[derive(Debug, Clone)]
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
            vec![
                FileNode::new_file("main.rs"),
                FileNode::new_file("lib.rs"),
            ],
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
                    FileNode::new_dir_with_children(
                        "utils",
                        vec![FileNode::new_file("helper.rs")],
                    ),
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
                vec![
                    FileNode::new_file("main.rs"),
                    FileNode::new_dir("utils"),
                ],
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
}
