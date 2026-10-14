//! 偽サーバのエージェントが、決めた回の要求を受けたときに、ディレクトリを中身ごと消す仕掛け
//! （変更の途中で root_dir ごと消えたときを作るため）

use std::path::PathBuf;
use std::sync::Mutex;

/// 数えた回のどこで消すか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// その回の要求を動かす前
    Before,
    /// その回の後、次に受けた要求（種類によらない）を動かす前
    AfterNext,
}

/// 何回目の要求のどこで、どのディレクトリを消すか
pub struct Removal {
    /// 数えるエージェントの要求の種類（"WriteFile" など）
    pub agent_request: &'static str,
    /// 何回目で消すか（1 から）
    pub occurrence: usize,
    pub when: When,
    pub dir: PathBuf,
    /// 消した後に、同じパスに通常ファイルを置くか
    pub replace_with_file: bool,
}

impl Removal {
    fn apply(&self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
        if self.replace_with_file {
            std::fs::write(&self.dir, "now a file\n").unwrap();
        }
    }
}

/// 仕掛けた消し方と、受けた数と、次で消すか
#[derive(Default)]
pub struct ArmedRemoval {
    armed: Mutex<Option<(Removal, usize, bool)>>,
}

impl ArmedRemoval {
    pub fn arm(&self, removal: Removal) {
        *self.armed.lock().unwrap() = Some((removal, 0, false));
    }

    /// エージェントが要求 `kind` を受けた
    pub fn on_agent_request(&self, kind: &str) {
        self.count(|removal| removal.agent_request == kind);
    }

    fn count(&self, matches: impl Fn(&Removal) -> bool) {
        let mut armed = self.armed.lock().unwrap();
        let Some((removal, seen, next)) = armed.as_mut() else {
            return;
        };
        if *next {
            removal.apply();
            *armed = None;
            return;
        }
        if !matches(removal) {
            return;
        }
        *seen += 1;
        if *seen != removal.occurrence {
            return;
        }
        match removal.when {
            When::Before => {
                removal.apply();
                *armed = None;
            }
            When::AfterNext => *next = true,
        }
    }
}

/// 決めた回の要求を動かす前に、ファイルの中身を大きさと更新時刻を変えずに書き換えるか、symlink を
/// 作り直す仕掛け（比べた後・変更の前に書き込み先が変わったときを作るため）
pub struct Rewrite {
    pub agent_request: &'static str,
    /// 何回目の要求の前に書き換えるか（1 から）
    pub occurrence: usize,
    pub path: PathBuf,
    pub change: RewriteKind,
}

/// 書き換え方
pub enum RewriteKind {
    /// 同じ大きさの中身にする（更新時刻は元のまま）
    SameSizeContent(Vec<u8>),
    /// symlink をこのリンク文字列で作り直す
    Repoint(String),
}

impl Rewrite {
    fn apply(&self) {
        match &self.change {
            RewriteKind::SameSizeContent(content) => {
                let modified = std::fs::metadata(&self.path).unwrap().modified().unwrap();
                assert_eq!(
                    std::fs::metadata(&self.path).unwrap().len(),
                    content.len() as u64
                );
                std::fs::write(&self.path, content).unwrap();
                std::fs::File::options()
                    .write(true)
                    .open(&self.path)
                    .unwrap()
                    .set_modified(modified)
                    .unwrap();
            }
            RewriteKind::Repoint(target) => {
                std::fs::remove_file(&self.path).unwrap();
                std::os::unix::fs::symlink(target, &self.path).unwrap();
            }
        }
    }
}

/// 仕掛けた書き換えと、受けた数
#[derive(Default)]
pub struct ArmedRewrite {
    armed: Mutex<Option<(Rewrite, usize)>>,
}

impl ArmedRewrite {
    pub fn arm(&self, rewrite: Rewrite) {
        *self.armed.lock().unwrap() = Some((rewrite, 0));
    }

    /// エージェントが要求 `kind` を受けた
    pub fn on_agent_request(&self, kind: &str) {
        let mut armed = self.armed.lock().unwrap();
        let Some((rewrite, seen)) = armed.as_mut() else {
            return;
        };
        if rewrite.agent_request != kind {
            return;
        }
        *seen += 1;
        if *seen == rewrite.occurrence {
            rewrite.apply();
            *armed = None;
        }
    }
}
