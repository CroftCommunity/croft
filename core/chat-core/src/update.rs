//! The pure reducer: `update(model, intent) -> (model, effects)`.
//!
//! Total and panic-free (no `unwrap`, no indexing): malformed or stale input is
//! dropped, never fatal — the tenant analogue of `croft-group`'s
//! hostile-frame survival.

use crate::model::{Effect, Intent, MessageLine, Model};

/// Apply `intent` to `model`, returning the next model and any effects.
#[must_use]
pub fn update(mut model: Model, intent: Intent) -> (Model, Vec<Effect>) {
    match intent {
        Intent::SelectGroup(group) => {
            model.selected_group = Some(group);
            // Switching groups resets to the group-level timeline.
            model.selected_channel = None;
            model.channels.clear();
            model.timeline.clear();
            (
                model,
                vec![Effect::LoadTimeline {
                    group,
                    channel: None,
                }],
            )
        }
        Intent::SelectChannel(channel) => match model.selected_group {
            Some(group) => {
                model.selected_channel = Some(channel);
                model.timeline.clear();
                (
                    model,
                    vec![Effect::LoadTimeline {
                        group,
                        channel: Some(channel),
                    }],
                )
            }
            // No group selected: ignore.
            None => (model, vec![]),
        },
        Intent::TypeChar(c) => {
            model.draft.push(c);
            (model, vec![])
        }
        Intent::Backspace => {
            model.draft.pop();
            (model, vec![])
        }
        Intent::SendMessage => {
            let body = model.draft.trim().to_string();
            match model.selected_group {
                Some(group) if !body.is_empty() => {
                    // Optimistic append so the sender sees their message
                    // immediately; the confirmed line replaces it on refresh.
                    model.timeline.push(MessageLine {
                        lamport: MessageLine::OPTIMISTIC,
                        author: "me".to_string(),
                        body: body.clone(),
                    });
                    let channel = model.selected_channel;
                    model.draft.clear();
                    (
                        model,
                        vec![Effect::Send {
                            group,
                            channel,
                            body,
                        }],
                    )
                }
                // No group selected or empty draft: nothing happens.
                _ => (model, vec![]),
            }
        }
        Intent::Refresh(snapshot) => {
            model.groups = snapshot.groups;
            model.channels = snapshot.channels;
            // Fork status is group-level; adopt it whenever the snapshot is for
            // the selected group (a contradiction must not be silently dropped).
            if snapshot.group == model.selected_group {
                model.fork = snapshot.fork;
            }
            // Only adopt the timeline if the snapshot is for the selected
            // group *and* channel; a stale/mismatched snapshot is dropped.
            if snapshot.group == model.selected_group
                && snapshot.channel == model.selected_channel
                && model.selected_group.is_some()
            {
                model.timeline = snapshot.timeline;
            }
            (model, vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GroupRef, Snapshot};
    use social_tree_core::model::GroupId;

    fn gid(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }

    #[test]
    fn send_message_emits_send_effect_and_optimistic_append() {
        let mut model = Model {
            selected_group: Some(gid(1)),
            ..Model::default()
        };
        for c in "hello".chars() {
            let (m, fx) = update(model, Intent::TypeChar(c));
            model = m;
            assert!(fx.is_empty(), "typing emits no effect");
        }

        let (model, effects) = update(model, Intent::SendMessage);
        assert_eq!(
            effects,
            vec![Effect::Send {
                group: gid(1),
                channel: None,
                body: "hello".to_string()
            }]
        );
        assert_eq!(model.timeline.len(), 1, "optimistic line appended");
        assert_eq!(model.timeline[0].body, "hello");
        assert_eq!(model.timeline[0].lamport, MessageLine::OPTIMISTIC);
        assert!(model.draft.is_empty(), "draft cleared after send");
    }

    #[test]
    fn send_with_no_group_or_empty_draft_is_a_noop() {
        // No selected group.
        let model = Model {
            draft: "hi".to_string(),
            ..Model::default()
        };
        let (model, effects) = update(model, Intent::SendMessage);
        assert!(effects.is_empty(), "no group selected -> no send");
        assert_eq!(model.draft, "hi", "draft preserved");

        // Selected group but whitespace-only draft.
        let model = Model {
            selected_group: Some(gid(2)),
            draft: "   ".to_string(),
            ..Model::default()
        };
        let (model, effects) = update(model, Intent::SendMessage);
        assert!(effects.is_empty(), "empty draft -> no send");
        assert!(
            model.timeline.is_empty(),
            "no optimistic line for empty body"
        );
    }

    #[test]
    fn select_group_clears_timeline_and_requests_load() {
        let model = Model {
            timeline: vec![MessageLine {
                lamport: 1,
                author: "x".into(),
                body: "old".into(),
            }],
            ..Model::default()
        };
        let (model, effects) = update(model, Intent::SelectGroup(gid(3)));
        assert_eq!(model.selected_group, Some(gid(3)));
        assert!(
            model.timeline.is_empty(),
            "switching group clears the old timeline"
        );
        assert_eq!(
            effects,
            vec![Effect::LoadTimeline {
                group: gid(3),
                channel: None
            }]
        );
    }

    #[test]
    fn refresh_for_wrong_group_is_dropped_not_applied() {
        let model = Model {
            selected_group: Some(gid(4)),
            timeline: vec![MessageLine {
                lamport: 1,
                author: "keep".into(),
                body: "keep".into(),
            }],
            ..Model::default()
        };
        // Snapshot is for a DIFFERENT group — must not replace the timeline.
        let snapshot = Snapshot {
            groups: vec![GroupRef {
                id: gid(4),
                title: "G4".into(),
                member_count: 2,
            }],
            channels: vec![],
            group: Some(gid(99)),
            channel: None,
            timeline: vec![MessageLine {
                lamport: 2,
                author: "wrong".into(),
                body: "wrong".into(),
            }],
            fork: None,
        };
        let (model, effects) = update(model, Intent::Refresh(snapshot));
        assert!(effects.is_empty());
        assert_eq!(model.timeline.len(), 1, "stale timeline dropped");
        assert_eq!(model.timeline[0].body, "keep");
        assert_eq!(model.groups.len(), 1, "group list still adopted");
    }

    #[test]
    fn refresh_for_selected_group_replaces_timeline() {
        let model = Model {
            selected_group: Some(gid(5)),
            ..Model::default()
        };
        let snapshot = Snapshot {
            groups: vec![],
            channels: vec![],
            group: Some(gid(5)),
            channel: None,
            timeline: vec![MessageLine {
                lamport: 7,
                author: "a".into(),
                body: "new".into(),
            }],
            fork: None,
        };
        let (model, _) = update(model, Intent::Refresh(snapshot));
        assert_eq!(model.timeline.len(), 1);
        assert_eq!(model.timeline[0].body, "new");
    }

    fn channel(seed: u8) -> social_tree_core::model::TypedId {
        social_tree_core::model::TypedId::new(
            social_tree_core::model::KindTag::ArtifactChat,
            social_tree_core::model::Hash::new([seed; 32]),
        )
    }

    #[test]
    fn select_channel_sets_channel_and_requests_channel_load() {
        let model = Model {
            selected_group: Some(gid(1)),
            timeline: vec![MessageLine {
                lamport: 1,
                author: "x".into(),
                body: "old".into(),
            }],
            ..Model::default()
        };
        let ch = channel(7);
        let (model, effects) = update(model, Intent::SelectChannel(ch));
        assert_eq!(model.selected_channel, Some(ch));
        assert!(
            model.timeline.is_empty(),
            "channel switch clears the timeline"
        );
        assert_eq!(
            effects,
            vec![Effect::LoadTimeline {
                group: gid(1),
                channel: Some(ch)
            }]
        );
    }

    #[test]
    fn select_channel_without_a_group_is_a_noop() {
        let (model, effects) = update(Model::default(), Intent::SelectChannel(channel(7)));
        assert!(model.selected_channel.is_none());
        assert!(effects.is_empty());
    }

    #[test]
    fn send_targets_the_selected_channel() {
        let model = Model {
            selected_group: Some(gid(1)),
            selected_channel: Some(channel(9)),
            draft: "hello".into(),
            ..Model::default()
        };
        let (_model, effects) = update(model, Intent::SendMessage);
        assert_eq!(
            effects,
            vec![Effect::Send {
                group: gid(1),
                channel: Some(channel(9)),
                body: "hello".to_string()
            }]
        );
    }

    #[test]
    fn select_group_resets_channel_selection() {
        let model = Model {
            selected_group: Some(gid(1)),
            selected_channel: Some(channel(9)),
            ..Model::default()
        };
        let (model, _) = update(model, Intent::SelectGroup(gid(2)));
        assert_eq!(
            model.selected_channel, None,
            "switching group clears channel"
        );
    }

    #[test]
    fn refresh_adopts_fork_status_for_the_selected_group() {
        let model = Model {
            selected_group: Some(gid(1)),
            ..Model::default()
        };
        let snapshot = Snapshot {
            group: Some(gid(1)),
            fork: Some("forked_from:dead".to_string()),
            ..Snapshot::default()
        };
        let (model, _) = update(model, Intent::Refresh(snapshot));
        assert_eq!(model.fork.as_deref(), Some("forked_from:dead"));
    }

    #[test]
    fn backspace_on_empty_draft_does_not_panic() {
        let model = Model::default();
        let (model, effects) = update(model, Intent::Backspace);
        assert!(model.draft.is_empty());
        assert!(effects.is_empty());
    }
}

#[cfg(test)]
mod p6_tests {
    use super::*;
    use crate::model::{MemberRow, Snapshot, Standing};
    use social_tree_core::model::{GroupId, PrincipalId};

    fn gid(seed: u8) -> GroupId {
        GroupId::new([seed; 32])
    }
    fn pid(seed: u8) -> PrincipalId {
        PrincipalId::new([seed; 32])
    }

    /// **The truthful membership panel: the snapshot's member rows adopt
    /// group-scoped, with standing carried as data** — Seated, pending
    /// resolution (E108's CONTESTED rendering), or voided (E116's
    /// "admission voided" legibility). The pond renders what the fold
    /// holds; it never manufactures a cleaner list.
    #[test]
    fn refresh_adopts_member_rows_for_the_selected_group() {
        let model = Model {
            selected_group: Some(gid(1)),
            ..Model::default()
        };
        let rows = vec![
            MemberRow { principal: pid(0xA), role: "owner".into(), standing: Standing::Seated },
            MemberRow { principal: pid(0xB), role: "member".into(), standing: Standing::PendingResolution },
            MemberRow { principal: pid(0xC), role: "member".into(), standing: Standing::Voided },
        ];
        let snapshot = Snapshot {
            group: Some(gid(1)),
            members: rows.clone(),
            ..Snapshot::default()
        };
        let (model, _) = update(model, Intent::Refresh(snapshot));
        assert_eq!(model.members, rows, "the panel is the fold's truth");

        // A stale snapshot for another group must not replace the panel.
        let stale = Snapshot {
            group: Some(gid(9)),
            members: vec![],
            ..Snapshot::default()
        };
        let (model, _) = update(model, Intent::Refresh(stale));
        assert_eq!(model.members.len(), 3, "stale member rows dropped");
    }

    /// **Mute is a personal annotation on the edge, never a group fact
    /// (E134): toggling emits the persist effect and never a Send.**
    #[test]
    fn toggle_mute_flips_and_emits_persist() {
        let model = Model::default();
        let (model, fx) = update(model, Intent::ToggleMute(pid(0xB)));
        assert!(model.muted.contains(&pid(0xB)));
        assert_eq!(fx, vec![Effect::PersistMuted(vec![pid(0xB)])]);

        let (model, fx) = update(model, Intent::ToggleMute(pid(0xB)));
        assert!(!model.muted.contains(&pid(0xB)), "toggle un-mutes");
        assert_eq!(fx, vec![Effect::PersistMuted(vec![])]);
    }

    /// **The startup seed: the shell hands back what it persisted.**
    #[test]
    fn set_muted_seeds_without_effects() {
        let (model, fx) = update(
            Model::default(),
            Intent::SetMuted(vec![pid(1), pid(2)]),
        );
        assert!(model.muted.contains(&pid(1)) && model.muted.contains(&pid(2)));
        assert!(fx.is_empty(), "seeding persists nothing");
    }
}
