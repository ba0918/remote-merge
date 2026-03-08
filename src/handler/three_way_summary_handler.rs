//! 3way サマリーパネルの開閉ロジック。

use crate::app::AppState;

/// W キーで 3way サマリーパネルを開く（diff_keys / tree_keys 両方から呼ばれる）
pub fn open_three_way_summary(state: &mut AppState) {
    state.open_three_way_summary();
}
