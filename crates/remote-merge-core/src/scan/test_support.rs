//! 規則の単体テストで使う、手で作った生の項目を答える集め方
use std::collections::HashMap;

use super::exclude::Exclude;
use super::outcome::{ListedKind, Listing, ScanError, ScanFailure, ScanRequest, ScanScope, Start};
use super::raw::{
    Answer, CollectError, Collector, DirId, LinkTarget, Question, RawEntry, RawKind, RawTime,
    Subtree, Unreached,
};

/// 問いのパスごとに決めた答えを返し、受けた問いを覚える
#[derive(Default)]
pub struct Fake {
    pub answers: HashMap<Vec<u8>, Answer>,
    pub asked: Vec<Question>,
    pub calls: usize,
}

impl Fake {
    pub fn with(mut self, path: &[u8], answer: Answer) -> Self {
        self.answers.insert(path.to_vec(), answer);
        self
    }

    pub fn asked_paths(&self) -> Vec<String> {
        self.asked
            .iter()
            .map(|q| String::from_utf8_lossy(&q.path).into_owned())
            .collect()
    }
}

impl Collector for Fake {
    fn collect(
        &mut self,
        questions: &[Question],
        _: &Exclude,
    ) -> Result<Vec<Answer>, CollectError> {
        self.calls += 1;
        questions
            .iter()
            .map(|q| {
                self.asked.push(q.clone());
                self.answers.get(&q.path).cloned().ok_or_else(|| {
                    CollectError::Failed(anyhow::anyhow!(
                        "unexpected question: {}",
                        String::from_utf8_lossy(&q.path)
                    ))
                })
            })
            .collect()
    }
}

pub fn id(ino: u64) -> DirId {
    DirId { dev: 1, ino }
}

fn entry(path: &[u8], kind: RawKind, ino: u64) -> RawEntry {
    RawEntry {
        path: path.to_vec(),
        kind,
        size: 3,
        mtime: RawTime::Read {
            secs: 100,
            nanos: 5,
            digits: 9,
        },
        mode: 0o644,
        id: id(ino),
    }
}

pub fn file(path: &[u8]) -> RawEntry {
    entry(path, RawKind::File, 0)
}

pub fn dir(path: &[u8], ino: u64) -> RawEntry {
    entry(path, RawKind::Directory, ino)
}

pub fn link(path: &[u8], target: &[u8]) -> RawEntry {
    let kind = RawKind::Symlink {
        target: Some(target.to_vec()),
    };
    entry(path, kind, 0)
}

pub fn subtree(start_ino: u64, entries: Vec<RawEntry>) -> Subtree {
    Subtree {
        start: dir(b"", start_ino),
        entries,
        unreadable: Vec::new(),
        unreadable_entries: Vec::new(),
        changed: Vec::new(),
    }
}

/// root_dir の答え。`entries` があれば下も集めた答えにする
pub fn root(entries: Option<Vec<RawEntry>>) -> Answer {
    Answer {
        entry: Ok(dir(b"", 1)),
        link: None,
        subtree: entries.map(|entries| subtree(1, entries)),
    }
}

pub fn root_with(tree: Subtree) -> Answer {
    Answer {
        entry: Ok(dir(b"", 1)),
        link: None,
        subtree: Some(tree),
    }
}

/// symlink の問いへの答え。リンク先がディレクトリなら `tree` にその下を渡す
pub fn link_answer(target: LinkTarget, tree: Option<Subtree>) -> Answer {
    Answer {
        entry: Ok(link(b"", b"t")),
        link: Some(target),
        subtree: tree,
    }
}

pub fn plain(entry: RawEntry, tree: Option<Subtree>) -> Answer {
    Answer {
        entry: Ok(entry),
        link: None,
        subtree: tree,
    }
}

pub fn unreached_answer(why: Unreached) -> Answer {
    Answer {
        entry: Err(why),
        link: None,
        subtree: None,
    }
}

pub fn whole(max_entries: usize) -> ScanRequest {
    ScanRequest {
        scope: ScanScope::Whole,
        max_entries,
    }
}

pub fn scope(scope: ScanScope, max_entries: usize) -> ScanRequest {
    ScanRequest { scope, max_entries }
}

pub fn exclude(patterns: &[&str]) -> Exclude {
    Exclude::new(&patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>())
}

pub fn run(
    fake: &mut Fake,
    exclude: &Exclude,
    request: &ScanRequest,
) -> Result<Listing, ScanError> {
    super::drive::scan(fake, exclude, request)
}

pub fn failure(result: Result<Listing, ScanError>) -> ScanFailure {
    match result {
        Err(ScanError::Failure(failure)) => failure,
        other => panic!("expected a scan failure, got {other:?}"),
    }
}

/// 一覧に載ったファイルと symlink のパス（ディレクトリを除く）
pub fn listed(listing: &Listing) -> Vec<String> {
    listing
        .entries
        .iter()
        .filter(|e| !matches!(e.kind, ListedKind::Directory))
        .map(|e| e.path.clone())
        .collect()
}

/// 指定したディレクトリの下だけの起点（ディレクトリごとに件数の組を分ける）
pub fn below(paths: &[&str]) -> ScanScope {
    let starts = paths
        .iter()
        .enumerate()
        .map(|(group, p)| Start::below(*p, group))
        .collect();
    ScanScope::Starts(starts)
}

pub fn values(paths: &[&str]) -> ScanScope {
    ScanScope::Starts(paths.iter().map(|p| Start::value(*p)).collect())
}
