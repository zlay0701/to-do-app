//! 记录级 LWW 合并（SPEC §9.2）：纯函数、零 IO，双端共用。
//!
//! 裁决只看 `updated_at` + `origin`，不比对内容 hash——规则保持可预测。
//! tombstone（`deleted_at` 非空）与普通记录走同一套规则，
//! 所以一条更新的修改可以把已删除的记录"复活"，反之亦然。

use crate::models::{Task, TodoList};

/// LWW 裁决结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LwwOutcome {
    /// 接受新记录（覆盖旧的）。
    Accept,
    /// 保留旧记录（拒绝新记录）。
    Reject,
    /// 两条记录在同步语义上相同，无需写入。
    Ignore,
}

/// 逐条裁决的核心规则。
///
/// * 新 `updated_at` 胜旧；旧不胜新。
/// * 时间相同且内容相同 → 忽略。
/// * 时间相同、内容不同 → `origin` 字典序大者胜。
/// * 时间相同、内容不同、`origin` 也相同 → 无法定序，保留旧的（确定性优先）。
///   同一设备同一毫秒写出两条不同内容本就不该发生，保留旧值可保证两端收敛到同一结果。
///
/// 返回 `(裁决, 是否记一条冲突日志）`：败方都要记（SPEC §9.2）。
pub fn decide(
    incoming_updated_at: i64,
    incoming_origin: &str,
    existing_updated_at: i64,
    existing_origin: &str,
    contents_equal: bool,
) -> (LwwOutcome, bool) {
    use std::cmp::Ordering::*;
    match incoming_updated_at.cmp(&existing_updated_at) {
        Greater => (LwwOutcome::Accept, false),
        Less => (LwwOutcome::Reject, true),
        Equal => {
            if contents_equal {
                (LwwOutcome::Ignore, false)
            } else {
                match incoming_origin.cmp(existing_origin) {
                    Greater => (LwwOutcome::Accept, true),
                    Less => (LwwOutcome::Reject, true),
                    Equal => (LwwOutcome::Ignore, false),
                }
            }
        }
    }
}

/// 除 `updated_at` / `origin` 之外的同步字段是否相同。
/// 这两个字段是裁决依据，不参与"内容相同"的判断。
pub fn lists_content_equal(a: &TodoList, b: &TodoList) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.color == b.color
        && a.sort_order == b.sort_order
        && a.deleted_at == b.deleted_at
        && a.created_at == b.created_at
}

pub fn tasks_content_equal(a: &Task, b: &Task) -> bool {
    a.id == b.id
        && a.list_id == b.list_id
        && a.title == b.title
        && a.notes == b.notes
        && a.status == b.status
        && a.priority == b.priority
        && a.due_at == b.due_at
        && a.completed_at == b.completed_at
        && a.sort_order == b.sort_order
        && a.deleted_at == b.deleted_at
        && a.created_at == b.created_at
}

/// 对两条列表记录做 LWW 裁决。
pub fn decide_list(incoming: &TodoList, existing: &TodoList) -> (LwwOutcome, bool) {
    decide(
        incoming.updated_at,
        &incoming.origin,
        existing.updated_at,
        &existing.origin,
        lists_content_equal(incoming, existing),
    )
}

/// 对两条任务记录做 LWW 裁决。
pub fn decide_task(incoming: &Task, existing: &Task) -> (LwwOutcome, bool) {
    decide(
        incoming.updated_at,
        &incoming.origin,
        existing.updated_at,
        &existing.origin,
        tasks_content_equal(incoming, existing),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Task, TodoList};

    fn list(updated_at: i64, origin: &str, name: &str, deleted_at: Option<i64>) -> TodoList {
        TodoList {
            id: "l1".to_string(),
            name: name.to_string(),
            color: "#378ADD".to_string(),
            sort_order: 0,
            deleted_at,
            updated_at,
            origin: origin.to_string(),
            created_at: 1000,
        }
    }

    fn task(updated_at: i64, origin: &str, title: &str, deleted_at: Option<i64>) -> Task {
        Task {
            id: "t1".to_string(),
            list_id: "l1".to_string(),
            title: title.to_string(),
            notes: String::new(),
            status: 0,
            priority: 0,
            due_at: None,
            completed_at: None,
            sort_order: 1.0,
            deleted_at,
            updated_at,
            origin: origin.to_string(),
            created_at: 1000,
        }
    }

    #[test]
    fn newer_wins() {
        let (o, c) = decide_list(&list(2000, "a", "新", None), &list(1000, "b", "旧", None));
        assert_eq!((o, c), (LwwOutcome::Accept, false));
    }

    #[test]
    fn older_does_not_win() {
        let (o, c) = decide_list(&list(1000, "a", "旧", None), &list(2000, "b", "新", None));
        assert_eq!((o, c), (LwwOutcome::Reject, true));
    }

    #[test]
    fn same_time_same_content_is_ignored() {
        let (o, c) = decide_task(&task(1000, "a", "同", None), &task(1000, "b", "同", None));
        assert_eq!((o, c), (LwwOutcome::Ignore, false));
    }

    #[test]
    fn same_time_different_content_larger_origin_wins() {
        // incoming origin 更大 → 接受，且败方记冲突
        let (o, c) = decide_task(
            &task(1000, "win-zz", "Y", None),
            &task(1000, "win-aa", "X", None),
        );
        assert_eq!((o, c), (LwwOutcome::Accept, true));
        // incoming origin 更小 → 拒绝，同样记冲突
        let (o, c) = decide_task(
            &task(1000, "win-aa", "X", None),
            &task(1000, "win-zz", "Y", None),
        );
        assert_eq!((o, c), (LwwOutcome::Reject, true));
    }

    #[test]
    fn tombstone_can_be_revived_by_newer_write() {
        // 服务端存着一条旧 tombstone，客户端拿更新的"活"记录来 → 复活
        let (o, c) = decide_task(
            &task(2000, "a", "复活", None),
            &task(1000, "b", "已删", Some(1000)),
        );
        assert_eq!((o, c), (LwwOutcome::Accept, false));
    }

    #[test]
    fn newer_tombstone_deletes() {
        // 更新的 tombstone 覆盖旧的活记录 → 删除传播
        let (o, c) = decide_task(
            &task(2000, "a", "已删", Some(2000)),
            &task(1000, "b", "活", None),
        );
        assert_eq!((o, c), (LwwOutcome::Accept, false));
    }
}
