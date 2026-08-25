//! `project(&Model) -> ChatView` — the total, deterministic model→view map.

use crate::model::{MessageLine, Model, Standing};
use crate::view::{
    ChannelNode, ChatView, GraphTreeView, GroupNode, MemberRowView, MembersPaneView,
    TimelineLineView, TimelineView, TreeRow,
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
            muted: m.author_principal.is_some_and(|p| model.muted.contains(&p)),
        })
        .collect();

    let member_rows = model
        .members
        .iter()
        .map(|m| MemberRowView {
            principal: m.principal,
            role: m.role.clone(),
            standing_label: match m.standing {
                Standing::Seated => String::new(),
                Standing::PendingResolution => "membership pending resolution".to_string(),
                Standing::Voided => "admission voided".to_string(),
            },
            muted: model.muted.contains(&m.principal),
        })
        .collect();

    ChatView {
        tree: GraphTreeView { rows },
        timeline: TimelineView { lines },
        draft: model.draft.clone(),
        fork: model.fork.clone(),
        members: MembersPaneView { rows: member_rows },
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
                    author_principal: None,
                    body: "first".into(),
                },
                MessageLine {
                    lamport: 2,
                    author: "b".into(),
                    author_principal: None,
                    body: "second".into(),
                },
                MessageLine {
                    lamport: 3,
                    author: "a".into(),
                    author_principal: None,
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
                    author_principal: None,
                    body: "confirmed".into(),
                },
                MessageLine {
                    lamport: MessageLine::OPTIMISTIC,
                    author: "me".into(),
                    author_principal: None,
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

#[cfg(test)]
mod p6_tests {
    use super::project;
    use crate::model::{MemberRow, MessageLine, Model, Standing};
    use social_tree_core::model::PrincipalId;

    fn pid(seed: u8) -> PrincipalId {
        PrincipalId::new([seed; 32])
    }

    /// **A muted author's lines render marked, never silently dropped** —
    /// hiding the fact of a message would be lying by omission; the shell
    /// collapses marked lines.
    #[test]
    fn muted_authors_lines_are_marked_not_dropped() {
        let mut model = Model::default();
        model.muted.insert(pid(0xB));
        model.timeline = vec![
            MessageLine {
                lamport: 1,
                author: "a".into(),
                author_principal: Some(pid(0xA)),
                body: "keep".into(),
            },
            MessageLine {
                lamport: 2,
                author: "b".into(),
                author_principal: Some(pid(0xB)),
                body: "muted".into(),
            },
        ];
        let view = project(&model);
        assert_eq!(view.timeline.lines.len(), 2, "nothing dropped");
        assert!(!view.timeline.lines[0].muted);
        assert!(
            view.timeline.lines[1].muted,
            "the muted author's line is marked"
        );
    }

    /// **The members pane projects standing as words the product committed
    /// to** — "pending resolution" for CONTESTED (E108), "admission
    /// voided" for the ceiling (E116) — plus the mute marker on the row.
    #[test]
    fn members_pane_carries_standing_and_mute() {
        let mut model = Model::default();
        model.muted.insert(pid(0xB));
        model.members = vec![
            MemberRow {
                principal: pid(0xA),
                role: "owner".into(),
                standing: Standing::Seated,
            },
            MemberRow {
                principal: pid(0xB),
                role: "member".into(),
                standing: Standing::PendingResolution,
            },
            MemberRow {
                principal: pid(0xC),
                role: "member".into(),
                standing: Standing::Voided,
            },
        ];
        let view = project(&model);
        let rows = &view.members.rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].standing_label, "");
        assert_eq!(rows[1].standing_label, "membership pending resolution");
        assert_eq!(rows[2].standing_label, "admission voided");
        assert!(rows[1].muted, "the mute marker rides the row");
        assert!(!rows[0].muted);
    }
}
