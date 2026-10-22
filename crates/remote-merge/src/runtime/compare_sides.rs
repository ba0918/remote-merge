//! 比べる側（左右と --ref の参照先）

use std::collections::BTreeMap;

use remote_merge_core::compare::Role;

use crate::side::Side;

/// 比べる側
#[derive(Debug, Clone)]
pub struct CompareSides {
    pub left: Side,
    pub right: Side,
    /// --ref の参照先
    pub reference: Option<Side>,
}

impl CompareSides {
    pub(crate) fn each(&self) -> Vec<(Role, &Side)> {
        let mut sides = vec![(Role::Left, &self.left), (Role::Right, &self.right)];
        if let Some(reference) = &self.reference {
            sides.push((Role::Ref, reference));
        }
        sides
    }

    pub(crate) fn by_role(&self) -> BTreeMap<Role, Side> {
        self.each()
            .into_iter()
            .map(|(role, side)| (role, side.clone()))
            .collect()
    }
}
