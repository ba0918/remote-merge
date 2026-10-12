//! 走査のスクリプトをこのマシンの sh と find で動かすテストの道具
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use remote_merge_core::scan::drive;
use remote_merge_core::scan::exclude::Exclude;
use remote_merge_core::scan::outcome::{ScanRequest, ScanScope};
use remote_merge_core::scan::raw::{Answer, Collector, Question, RawEntry, RawTime};
use remote_merge_core::scan::this_machine::MachineCollector;

pub fn exclude(patterns: &[&str]) -> Exclude {
    Exclude::new(&patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>())
}

/// スクリプトを sh で動かして答えを読む（SSH の経路と同じく、空に見えたディレクトリには
/// もう一度尋ねる）
pub fn over_shell(
    root: &Path,
    questions: &[Question],
    exclude: &Exclude,
) -> Result<Vec<Answer>, String> {
    let run = |script: &str| {
        shell_command(script)
            .output()
            .map(|output| (output.stdout, output.stderr))
    };
    crate::scan_collect::collect_with(
        root.as_os_str().as_bytes(),
        questions,
        exclude,
        crate::batch_read::SSH_BATCH_MAX_COMMAND_LEN,
        run,
    )
    .map_err(|failure| match failure {
        crate::scan_collect::CollectFailure::Run(error) => error.to_string(),
        crate::scan_collect::CollectFailure::Read(message) => message,
    })
}

/// スクリプトを動かすシェル。REMOTE_MERGE_TEST_SCAN_IMAGE にコンテナのイメージ（centos:5 など）を
/// 置くと、一時ディレクトリを同じパスに見せたそのコンテナの sh と find で動かす（古い find で
/// 確かめるための手段で、CI では使わない）
pub fn shell_command(script: &str) -> Command {
    shell_command_with_path(script, None)
}

/// [`shell_command`] の、`bin` を PATH の先頭に置いたもの（find が -exec で起こす sh を差し替える）
pub fn shell_command_with_path(script: &str, bin: Option<&Path>) -> Command {
    let image = std::env::var("REMOTE_MERGE_TEST_SCAN_IMAGE").unwrap_or_default();
    let path = bin.map(|bin| format!("{}:/usr/bin:/bin", bin.to_str().unwrap()));
    if image.is_empty() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        if let Some(path) = &path {
            command.env("PATH", path);
        }
        return command;
    }
    let temp = std::env::temp_dir();
    let temp = temp.to_str().unwrap();
    let user = format!("{}:{}", unsafe { libc::getuid() }, unsafe {
        libc::getgid()
    });
    let mut command = Command::new("docker");
    command
        .args(["run", "--rm", "-u", &user, "-v"])
        .arg(format!("{temp}:{temp}"));
    if let Some(path) = &path {
        command.args(["-e", &format!("PATH={path}")]);
    }
    command.args([image.as_str(), "/bin/sh", "-c", script]);
    command
}

pub fn on_machine(root: &Path, questions: &[Question], exclude: &Exclude) -> Vec<Answer> {
    MachineCollector::new(root.to_path_buf())
        .collect(questions, exclude)
        .unwrap()
}

/// 秒しか出さない find（findutils 4.2.27）のコンテナで動かしたときは、秒未満を比べない
/// （FLAG-scan-015）
pub fn seconds_only() -> bool {
    std::env::var("REMOTE_MERGE_TEST_SCAN_IMAGE").is_ok_and(|image| !image.is_empty())
}

/// 経路で違いうる欄をそろえる。find の %m は種類のビットを出さない
pub fn comparable(mut answers: Vec<Answer>) -> Vec<Answer> {
    let fix = |entry: &mut RawEntry| {
        entry.mode &= 0o7777;
        if let RawTime::Read { digits, nanos, .. } = &mut entry.mtime {
            *digits = 9;
            if seconds_only() {
                *nanos = 0;
            }
        }
    };
    for answer in &mut answers {
        if let Ok(entry) = &mut answer.entry {
            fix(entry);
        }
        if let Some(tree) = &mut answer.subtree {
            fix(&mut tree.start);
            tree.entries.iter_mut().for_each(fix);
            tree.entries.sort_by(|a, b| a.path.cmp(&b.path));
            tree.unreadable.sort();
        }
    }
    answers
}

pub fn question(path: &[u8], descend: bool) -> Question {
    Question {
        path: path.to_vec(),
        descend,
    }
}

/// 名前に TAB・改行・"\"・UTF-8 でないバイトを含む項目、ディレクトリへの symlink、リンク切れ、
/// 循環するリンク、途中がファイルのリンク、exclude に当たるディレクトリを置いた root_dir
pub fn tricky_root() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("d/sub")).unwrap();
    fs::write(root.join("d/sub/f.txt"), "abc").unwrap();
    for name in [
        &b"x\ty.txt"[..],
        b"p\nq.txt",
        b"b\\c.txt",
        b"bad\xff.txt",
        b"it's",
    ] {
        fs::write(root.join(OsStr::from_bytes(name)), "n").unwrap();
    }
    symlink("d", root.join("to_dir")).unwrap();
    symlink("nope", root.join("dangling")).unwrap();
    symlink("loop", root.join("loop")).unwrap();
    symlink("d/sub/f.txt/x", root.join("through_file")).unwrap();
    symlink("end\n", root.join("nl_target")).unwrap();
    fs::create_dir_all(root.join("cache/deep")).unwrap();
    fs::write(root.join("cache/deep/c.txt"), "c").unwrap();
    temp
}

/// スクリプトを sh で動かす集め方
pub struct Shell(pub std::path::PathBuf);

impl Collector for Shell {
    fn collect(
        &mut self,
        questions: &[Question],
        exclude: &Exclude,
    ) -> Result<Vec<Answer>, remote_merge_core::scan::raw::CollectError> {
        over_shell(&self.0, questions, exclude)
            .map_err(|e| remote_merge_core::scan::raw::CollectError(anyhow::anyhow!(e)))
    }
}

pub fn scan_with(collector: &mut dyn Collector, rule: &Exclude, scope: ScanScope) -> String {
    let request = ScanRequest {
        scope,
        max_entries: 100,
    };
    let result = drive::scan(collector, rule, &request).map(|mut listing| {
        for entry in &mut listing.entries {
            entry.mtime_digits = 9;
            if seconds_only() {
                entry.mtime_nanos = 0;
            }
        }
        listing
    });
    format!("{result:?}")
}
