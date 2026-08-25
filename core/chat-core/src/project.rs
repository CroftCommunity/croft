//! `project(&Model) -> ChatView` — the total, deterministic model→view map.

use crate::model::{MessageLine, Model};
use crate::view::{
    ChannelNode, ChatView, GraphTreeView, GroupNode, TimelineLineView, TimelineView, TreeRow,
};

/// Project the model into the renderable view. Total and order-preserving:
/// the timeline lines come out in model order; the tree marks the selected group.
#[must_use]
pub fn project(model: &Model) -> ChatView {
    let mut rows = Vec::new();
    for g in &model.groups {
        let selected = model.selected_group == Some(g.id);
        rows.push(TreeRow::Group(GroupNode {
            id: g.id,
            title: g.title.clone(),
            member_count: g.member_count,
            selected,
        }));
        // The selected group reveals its channels, nested beneath it.
        if selected {
            for c in &model.channels {
                rows.push(TreeRow::Channel(ChannelNode {
                    id: c.id,
                    name: c.name.clone(),
                    selected: model.selected_channel == Some(c.id),
                }));
            }
        }
    }

    let lines = model
        .timeline
        .iter()
        .map(|m| TimelineLineView {
            author: m.author.clone(),
            body: m.body.clone(),
            pending: m.lamport == MessageLine::OPTIMISTIC,
        })
        .collect();

    ChatView {
        tree: GraphTreeView { rows },
        timeline: TimelineView { lines },
        draft: model.draft.clone(),
        fork: model.fork.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::project;
    use crate::model::{GroupRef, MessageLine, Model};
    use social_tree_core::model::GroupId;

    fn gid(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    #[test]
    fn timeline_projects_n_lines_in_order() {
        let model = Model {
            timeline: vec![
                MessageLine {
                    lamport: 1,
                    author: "a".into(),
                    body: "first".into(),
                },
                MessageLine {
                    lamport: 2,
                    author: "b".into(),
                    body: "second".into(),
                },
                MessageLine {
                    lamport: 3,
                    author: "a".into(),
                    body: "third".into(),
                },
            ],
            ..Model::default()
        };
        let view = project(&model);
        assert_eq!(view.timeline.lines.len(), 3);
        let bodies: Vec<&str> = view
            .timeline
            .lines
            .iter()
            .map(|l| l.body.as_str())
            .collect();
        assert_eq!(bodies, vec!["first", "second", "third"], "order preserved");
    }

    #[test]
    fn optimistic_line_is_marked_pending() {
        let model = Model {
            timeline: vec![
                MessageLine {
                    lamport: 1,
                    author: "a".into(),
                    body: "confirmed".into(),
                },
                MessageLine {
                    lamport: MessageLine::OPTIMISTIC,
                    author: "me".into(),
                    body: "sending".into(),
                },
            ],
            ..Model::default()
        };
        let view = project(&model);
        assert!(
            !view.timeline.lines[0].pending,
            "confirmed line not pending"
        );
        assert!(view.timeline.lines[1].pending, "optimistic line is pending");
    }

    #[test]
    fn tree_marks_selected_group_and_reflects_membership() {
        use crate::view::TreeRow;
        let model = Model {
            groups: vec![
                GroupRef {
                    id: gid(1),
                    title: "Alpha".into(),
                    member_count: 3,
                },
                GroupRef {
                    id: gid(2),
                    title: "Beta".into(),
                    member_count: 1,
                },
            ],
            selected_group: Some(gid(2)),
            ..Model::default()
        };
        let view = project(&model);
        // Two group rows (no channels loaded), Beta selected.
        let groups: Vec<_> = view
            .tree
            .rows
            .iter()
            .filter_map(|r| match r {
                TreeRow::Group(g) => Some(g),
                TreeRow::Channel(_) => None,
            })
            .collect();
        assert_eq!(groups.len(), 2);
        assert!(!groups[0].selected, "Alpha not selected");
        assert!(groups[1].selected, "Beta selected");
        assert_eq!(groups[0].member_count, 3);
    }

    #[test]
    fn selected_group_channels_are_nested_after_it() {
        use crate::model::ChannelRef;
        use crate::view::TreeRow;
        use social_tree_core::model::{Hash, KindTag, TypedId};
        let ch = |s: u8| TypedId::new(KindTag::ArtifactChat, Hash::new([s; 32]));
        let model = Model {
            groups: vec![GroupRef {
                id: gid(1),
                title: "Alpha".into(),
                member_count: 1,
            }],
            selected_group: Some(gid(1)),
            channels: vec![
                ChannelRef {
                    id: ch(10),
                    name: "general".into(),
                },
                ChannelRef {
                    id: ch(11),
                    name: "photos".into(),
                },
            ],
            selected_channel: Some(ch(11)),
            ..Model::default()
        };
        let rows = project(&model).tree.rows;
        // group row, then two channel rows.
        assert!(matches!(rows[0], TreeRow::Group(_)));
        let chan_rows: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                TreeRow::Channel(c) => Some(c),
                TreeRow::Group(_) => None,
            })
            .collect();
        assert_eq!(chan_rows.len(), 2);
        assert!(chan_rows.iter().any(|c| c.name == "photos" && c.selected));
        assert!(chan_rows.iter().any(|c| c.name == "general" && !c.selected));
    }

    #[test]
    fn draft_is_carried_into_the_view() {
        let model = Model {
            draft: "typing…".into(),
            ..Model::default()
        };
        assert_eq!(project(&model).draft, "typing…");
    }
}
