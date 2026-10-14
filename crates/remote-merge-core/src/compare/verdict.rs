//! 判定の値（docs/ir/compare/judgement.md の TBL-compare-001・002）と、判定できない理由
//! （docs/ir/compare/reading.md の REQ-compare-014）

use std::fmt;

use crate::op_failure::OpFailure;

/// 比べる側
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Left,
    Right,
    /// --ref の参照先
    Ref,
    /// rollback の戻す先（rollback は戻す先の一つの側だけを比べる）
    Target,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Ref => "ref",
            Self::Target => "target",
        }
    }
}

/// 組の二つの側のどちらか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Present {
    First,
    Second,
}

/// 一つの側の読み込みの失敗
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideFailure {
    pub role: Role,
    pub failure: OpFailure,
}

/// 判定できない理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undecidable {
    /// どちらかの名前が UTF-8 として読めない
    UnreadableName,
    /// どちらかがその他の種類（REQ-compare-009）。値はその他の種類だった側
    NotRegularFile(Vec<Role>),
    /// どちらかがリンク先を辿れない symlink（docs/ir/scan/directory-links.md の REQ-scan-018）。
    /// 値はそうだった側
    UnfollowableLink(Vec<Role>),
    /// 中身かハッシュの読み込みが `接続断` 以外の失敗になった側（REQ-compare-012）
    ReadFailed(Vec<SideFailure>),
}

impl fmt::Display for Undecidable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnreadableName => f.write_str("the name is not valid UTF-8"),
            Self::NotRegularFile(roles) => {
                write_sides(f, crate::machine_ops::NOT_A_REGULAR_FILE, roles)
            }
            Self::UnfollowableLink(roles) => {
                write_sides(f, "cannot follow the symlink target", roles)
            }
            Self::ReadFailed(failures) => {
                let mut sides: Vec<&SideFailure> = failures.iter().collect();
                sides.sort_by_key(|side| side.role);
                let causes: Vec<String> = sides
                    .iter()
                    .map(|side| format!("{}: {}", side.role.name(), side.failure))
                    .collect();
                write!(f, "read failed: {}", causes.join("; "))
            }
        }
    }
}

/// `heading` に続けて、側の名前を決まった順に "; " でつないで書く
fn write_sides(f: &mut fmt::Formatter<'_>, heading: &str, roles: &[Role]) -> fmt::Result {
    let mut roles = roles.to_vec();
    roles.sort();
    let names: Vec<&str> = roles.iter().map(|role| role.name()).collect();
    write!(f, "{heading}: {}", names.join("; "))
}

/// 二つの側の組の判定
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    BothAbsent,
    Undecidable(Undecidable),
    /// 片側にだけある（値はある側）
    OneSided(Present),
    /// 種類（通常ファイル・ディレクトリ・末尾の symlink）が違う
    KindDiffers,
    Same,
    Differs,
}

/// 状態の組からの判定。中身で決める組は、ハッシュを読んでから TBL-compare-002 で決める
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judgement {
    Decided(Verdict),
    ByContent,
}
