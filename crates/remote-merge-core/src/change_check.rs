//! 変更の最中の `接続断` の後に、書き込み先を読んで確かめた結果の判定
//! （docs/ir/ssh/reconnect.md の REQ-ssh-022）。読み書きはしない。

use crate::machine_ops::PermissionMode;
use crate::scan::outcome::{LinkPoint, ListedEntry, ListedKind};
use crate::sha256::hex_of_bytes;

/// 変更の前の、そのパスの状態。書き込みの確かめに使う
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Before {
    /// なかった
    Absent,
    /// この中身の通常のファイルだった
    Content(Vec<u8>),
    /// この SHA-256（小文字の16進）の中身の通常のファイルだった（バックアップを取らない rollback が
    /// 書き戻す前にまとめて取る）
    Hash(String),
    /// 書く前の中身を持っていない
    Unknown,
}

/// 確かめた変更
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    /// 中身の書き込み
    Write {
        before: &'a Before,
        content: &'a [u8],
    },
    /// symlink の作成。どの呼び出し元も、ないパスに作る
    Symlink {
        target: &'a str,
    },
    Remove,
    /// 権限の変更。0o777 を超える値はどの経路も変えずに拒むので、変えたことにはならない
    Chmod {
        mode: u32,
    },
}

/// 確かめで読んだ、そのパスの今の状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    Absent,
    /// 通常のファイルと、その中身（権限の変更の確かめでは中身を読まない）
    File(Vec<u8>),
    /// 末尾の symlink と、そのリンク先
    Link(String),
    /// 権限の変更の確かめで読んだ、0o7777 の範囲の権限と、ディレクトリ（かディレクトリへの
    /// symlink）か
    Mode {
        mode: u32,
        directory: bool,
    },
}

/// 確かめた結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 書けた・消せた・変えた
    Done,
    /// 書いていない・消していない・変えていない
    NotDone,
    /// 書きかけ（書く前とも書こうとしたものとも違う）
    Partial,
    /// 書く前の状態を持っていないので、書いていないか書きかけかを分けられない
    Undetermined,
}

/// 今の状態から、変更が行われたかを決める
pub fn judge(change: &Change<'_>, observed: &Observed) -> Outcome {
    match change {
        Change::Write { before, content } => {
            if matches!(observed, Observed::File(now) if now == content) {
                return Outcome::Done;
            }
            match (before, observed) {
                (Before::Absent, Observed::Absent) => Outcome::NotDone,
                (Before::Content(previous), Observed::File(now)) if previous == now => {
                    Outcome::NotDone
                }
                (Before::Hash(previous), Observed::File(now)) if *previous == hex_of_bytes(now) => {
                    Outcome::NotDone
                }
                (Before::Unknown, _) => Outcome::Undetermined,
                _ => Outcome::Partial,
            }
        }
        Change::Symlink { target } => match observed {
            Observed::Link(now) if now == target => Outcome::Done,
            Observed::Absent => Outcome::NotDone,
            Observed::Link(_) | Observed::File(_) | Observed::Mode { .. } => Outcome::Partial,
        },
        Change::Remove => match observed {
            Observed::Absent => Outcome::Done,
            Observed::File(_) | Observed::Link(_) | Observed::Mode { .. } => Outcome::NotDone,
        },
        Change::Chmod { mode } => match observed {
            // ディレクトリはすでに付いていた setuid・setgid を残す（REQ-paths-018）
            Observed::Mode {
                mode: now,
                directory,
            } if PermissionMode::new(*mode)
                .is_ok_and(|mode| mode.resulting(*now, *directory) == *now) =>
            {
                Outcome::Done
            }
            Observed::Absent | Observed::File(_) | Observed::Link(_) | Observed::Mode { .. } => {
                Outcome::NotDone
            }
        },
    }
}

/// 打ち切っていない親の子の一覧 `entries` から、root_dir からのパス `path` の子の権限を読む
pub fn mode_in_listing(entries: &[ListedEntry], path: &str) -> Observed {
    entries
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| Observed::Mode {
            mode: entry.permissions,
            directory: matches!(
                entry.kind,
                ListedKind::Directory
                    | ListedKind::Symlink {
                        points_to: LinkPoint::Directory,
                        ..
                    }
            ),
        })
        .unwrap_or(Observed::Absent)
}

/// 確かめた結果が分かる失敗の文言。`cause` は最初の `接続断` の原因
pub fn describe(change: &Change<'_>, outcome: Outcome, observed: &Observed, cause: &str) -> String {
    let what = match (change, outcome) {
        (_, Outcome::Done) => "done".to_string(),
        (Change::Write { .. } | Change::Symlink { .. }, Outcome::NotDone) => {
            "not written: after reconnecting, the destination was confirmed unchanged".to_string()
        }
        (_, Outcome::Partial) => "partially written: after reconnecting, the destination matched \
             neither its previous nor the new content; it was not restored automatically"
            .to_string(),
        (_, Outcome::Undetermined) => "result unknown: after reconnecting, the destination did \
             not match the new content, and its previous content was not kept, so it cannot be \
             told whether it was left unchanged or partially written"
            .to_string(),
        (Change::Remove, Outcome::NotDone) => {
            "not deleted: after reconnecting, the file was confirmed to still exist".to_string()
        }
        (Change::Chmod { .. }, Outcome::NotDone) => match observed {
            Observed::Mode { mode: now, .. } => format!(
                "permissions not changed: after reconnecting, the mode was confirmed to be {now:o}"
            ),
            Observed::Absent | Observed::File(_) | Observed::Link(_) => {
                "permissions not changed: after reconnecting, the path was not found".to_string()
            }
        },
    };
    format!("{what} (the connection was lost: {cause})")
}

/// 確かめられなかったときの文言（つなぎ直しか確かめる読み込みが失敗した）
pub fn describe_unconfirmed(cause: &str, reason: &str) -> String {
    format!(
        "result unknown: the connection was lost and the destination could not be confirmed \
         ({reason}) (the connection was lost: {cause})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> Observed {
        Observed::File(text.as_bytes().to_vec())
    }

    #[test]
    fn a_write_is_judged_against_the_new_and_the_previous_content() {
        let previous = Before::Content(b"old".to_vec());
        let old_hash = Before::Hash(hex_of_bytes(b"old"));
        let cases = [
            (&previous, file("new"), Outcome::Done),
            (&previous, file("old"), Outcome::NotDone),
            (&previous, file("ne"), Outcome::Partial),
            (&previous, Observed::Absent, Outcome::Partial),
            (&previous, Observed::Link("x".into()), Outcome::Partial),
            (&Before::Absent, Observed::Absent, Outcome::NotDone),
            (&Before::Absent, file("new"), Outcome::Done),
            (&Before::Absent, file(""), Outcome::Partial),
            (&Before::Unknown, file("new"), Outcome::Done),
            (&Before::Unknown, file("old"), Outcome::Undetermined),
            (&old_hash, file("new"), Outcome::Done),
            (&old_hash, file("old"), Outcome::NotDone),
            (&old_hash, file("ne"), Outcome::Partial),
            (&old_hash, Observed::Absent, Outcome::Partial),
        ];
        for (before, observed, expected) in cases {
            let change = Change::Write {
                before,
                content: b"new",
            };
            assert_eq!(
                judge(&change, &observed),
                expected,
                "{before:?} {observed:?}"
            );
        }
    }

    #[test]
    fn a_symlink_a_removal_and_a_permission_change_are_judged_by_the_current_state() {
        let link = Change::Symlink { target: "t" };
        assert_eq!(judge(&link, &Observed::Link("t".into())), Outcome::Done);
        assert_eq!(judge(&link, &Observed::Absent), Outcome::NotDone);
        assert_eq!(judge(&link, &Observed::Link("u".into())), Outcome::Partial);
        assert_eq!(judge(&link, &file("t")), Outcome::Partial);

        assert_eq!(judge(&Change::Remove, &Observed::Absent), Outcome::Done);
        assert_eq!(judge(&Change::Remove, &file("x")), Outcome::NotDone);
        assert_eq!(
            judge(&Change::Remove, &Observed::Link("t".into())),
            Outcome::NotDone
        );

        let file = |mode| Observed::Mode {
            mode,
            directory: false,
        };
        let chmod = Change::Chmod { mode: 0o600 };
        assert_eq!(judge(&chmod, &file(0o600)), Outcome::Done);
        assert_eq!(judge(&chmod, &file(0o644)), Outcome::NotDone);
        assert_eq!(judge(&chmod, &Observed::Absent), Outcome::NotDone);
    }

    #[test]
    fn a_directory_permission_change_is_judged_with_its_setuid_and_setgid_kept() {
        let chmod = Change::Chmod { mode: 0o775 };
        let cases = [
            (0o2775, true, Outcome::Done),
            (0o6775, true, Outcome::Done),
            (0o775, true, Outcome::Done),
            (0o3775, true, Outcome::NotDone),
            (0o2755, true, Outcome::NotDone),
            (0o2775, false, Outcome::NotDone),
        ];
        for (mode, directory, expected) in cases {
            assert_eq!(
                judge(&chmod, &Observed::Mode { mode, directory }),
                expected,
                "{mode:o} {directory}"
            );
        }
    }

    #[test]
    fn a_mode_is_read_from_the_listing_and_a_name_missing_from_it_is_absent() {
        let entry = |path: &str, kind: ListedKind, permissions: u32| ListedEntry {
            path: path.to_string(),
            kind,
            size: 0,
            mtime_secs: 0,
            mtime_nanos: 0,
            mtime_digits: 9,
            permissions,
        };
        let entries = [
            entry("p/d", ListedKind::Directory, 0o2775),
            entry("p/f", ListedKind::File, 0o644),
        ];
        assert_eq!(
            mode_in_listing(&entries, "p/d"),
            Observed::Mode {
                mode: 0o2775,
                directory: true
            }
        );
        assert_eq!(
            mode_in_listing(&entries, "p/f"),
            Observed::Mode {
                mode: 0o644,
                directory: false
            }
        );
        assert_eq!(mode_in_listing(&entries, "p/x"), Observed::Absent);
    }

    #[test]
    fn each_confirmed_result_reads_differently_and_keeps_the_cause() {
        let before = Before::Content(b"old".to_vec());
        let write = Change::Write {
            before: &before,
            content: b"new",
        };
        let texts = [
            describe(&write, Outcome::NotDone, &file("old"), "eof"),
            describe(&write, Outcome::Partial, &file("ne"), "eof"),
            describe(&write, Outcome::Undetermined, &file("old"), "eof"),
            describe(&Change::Remove, Outcome::NotDone, &file("x"), "eof"),
            describe(
                &Change::Chmod { mode: 0o600 },
                Outcome::NotDone,
                &Observed::Mode {
                    mode: 0o644,
                    directory: false,
                },
                "eof",
            ),
            describe_unconfirmed("eof", "could not reconnect"),
        ];
        let words = [
            "not written",
            "partially written",
            "result unknown",
            "not deleted",
            "permissions not changed",
            "result unknown",
        ];
        for (text, word) in texts.iter().zip(words) {
            assert!(text.contains(word) && text.contains("eof"), "{text}");
        }
        assert!(texts[4].contains("644"), "{}", texts[4]);
        assert!(texts[5].contains("could not reconnect"), "{}", texts[5]);
    }
}
