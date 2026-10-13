//! 偽サーバが、決めた回の書き込みを受けたときに、それを動かす前にディレクトリを中身ごと消す仕掛け
//! （書いている間に root_dir ごと消えたときを作るため）

use std::path::PathBuf;
use std::sync::Mutex;

use super::server_faults::SshCommand;

/// 何回目の書き込みの前に、どのディレクトリを消すか
pub struct Removal {
    /// 数えるエージェントの要求の種類（"WriteFile" など）
    pub agent_request: &'static str,
    /// 数える SSH のコマンドの種類
    pub ssh_command: SshCommand,
    /// 何回目で消すか（1 から）
    pub occurrence: usize,
    pub dir: PathBuf,
}

/// 仕掛けた消し方と、受けた数
#[derive(Default)]
pub struct ArmedRemoval {
    armed: Mutex<Option<(Removal, usize)>>,
}

impl ArmedRemoval {
    pub fn arm(&self, removal: Removal) {
        *self.armed.lock().unwrap() = Some((removal, 0));
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
        let Some((removal, seen)) = armed.as_mut() else {
            return;
        };
        if !matches(removal) {
            return;
        }
        *seen += 1;
        if *seen == removal.occurrence {
            std::fs::remove_dir_all(&removal.dir).unwrap();
        }
    }
}
