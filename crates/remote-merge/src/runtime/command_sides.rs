//! コマンドが比べる側につなぐことと、出力に示す側の名前と root_dir

use super::core::CoreRuntime;
use crate::service::types::SourceInfo;
use crate::side::Side;

impl CoreRuntime {
    /// 比べる側につなぐ。ローカルと、ローカルに置き換えたサーバはつながない
    pub fn connect_side(&mut self, side: &Side) -> anyhow::Result<()> {
        match side {
            Side::Local => Ok(()),
            Side::Remote(name) if self.local_override(name).is_some() => Ok(()),
            Side::Remote(name) => self.connect_if_needed(name),
        }
    }

    /// 出力に示す側の名前（"label"）と root_dir（"root"。リモートは "サーバ名:パス"）
    pub fn source_info(&self, side: &Side) -> SourceInfo {
        SourceInfo {
            label: side.display_name().to_string(),
            root: super::target_io::shown_root(side, self),
        }
    }
}
