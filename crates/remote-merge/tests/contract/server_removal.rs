//! 偽サーバが、決めた回の要求かコマンドを受けたときに、ディレクトリを中身ごと消す仕掛け
//! （変更の途中で root_dir ごと消えたときを作るため）

use std::path::PathBuf;
use std::sync::Mutex;

use super::server_faults::SshCommand;

/// 数えた回のどこで消すか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// その回の要求かコマンドを動かす前
    Before,
    /// その回の後、次に受けた要求かコマンド（種類によらない）を動かす前
    AfterNext,
}

/// 何回目の要求かコマンドのどこで、どのディレクトリを消すか
pub struct Removal {
    /// 数えるエージェントの要求の種類（"WriteFile" など）
    pub agent_request: &'static str,
    /// 数える SSH のコマンドの種類
    pub ssh_command: SshCommand,
    /// 何回目で消すか（1 から）
    pub occurrence: usize,
    pub when: When,
    pub dir: PathBuf,
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

    /// SSH のコマンド `script` を受けた
    pub fn on_ssh_command(&self, script: &str) {
        self.count(|removal| removal.ssh_command.matches(script));
    }

    fn count(&self, matches: impl Fn(&Removal) -> bool) {
        let mut armed = self.armed.lock().unwrap();
        let Some((removal, seen, next)) = armed.as_mut() else {
            return;
        };
        if *next {
            std::fs::remove_dir_all(&removal.dir).unwrap();
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
                std::fs::remove_dir_all(&removal.dir).unwrap();
                *armed = None;
            }
            When::AfterNext => *next = true,
        }
    }
}
