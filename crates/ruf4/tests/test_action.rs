// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Round-trip tests locking the action/key name tables. These guard the
//! data-table consolidation in `action.rs`: `action_str`/`action_label` share one
//! match, `parse_action` is derived from `action_str`, and `key_display_name` /
//! `parse_key_name` share one `KEY_NAMES` table.

use std::collections::HashSet;

use ruf4::action::{
    ALL_ACTIONS, action_label, action_str, default_bindings, key_display_name, parse_action,
    parse_key_name,
};
use ruf4_tui::input::{kbmod, vk};

#[test]
fn action_str_round_trips_for_every_action() {
    for &a in ALL_ACTIONS {
        let s = action_str(a);
        assert!(!s.is_empty(), "empty action_str for {a:?}");
        assert_eq!(parse_action(s), Some(a), "round-trip failed for {a:?}");
    }
}

#[test]
fn action_strings_and_labels_are_unique() {
    let mut strs = HashSet::new();
    let mut labels = HashSet::new();
    for &a in ALL_ACTIONS {
        assert!(strs.insert(action_str(a)), "duplicate action_str: {a:?}");
        assert!(
            labels.insert(action_label(a)),
            "duplicate action_label: {a:?}"
        );
    }
}

#[test]
fn all_actions_has_no_duplicates() {
    // `Action` is not `Hash`; compare via the unique stable strings instead.
    let mut seen: Vec<&str> = Vec::new();
    for &a in ALL_ACTIONS {
        let s = action_str(a);
        assert!(
            !seen.contains(&s),
            "ALL_ACTIONS contains a duplicate: {a:?}"
        );
        seen.push(s);
    }
}

#[test]
fn parse_action_rejects_unknown() {
    assert_eq!(parse_action("definitely_not_an_action"), None);
    assert_eq!(parse_action(""), None);
}

#[test]
fn every_default_binding_action_is_listed() {
    // If a bound action is missing from ALL_ACTIONS it would silently fail to
    // parse from settings; catch that here.
    for b in default_bindings() {
        assert!(
            ALL_ACTIONS.contains(&b.action),
            "default binding action not in ALL_ACTIONS: {:?}",
            b.action
        );
    }
}

#[test]
fn key_names_round_trip() {
    let keys = [
        vk::F1,
        vk::F12,
        vk::UP,
        vk::DOWN,
        vk::LEFT,
        vk::RIGHT,
        vk::PRIOR,
        vk::NEXT,
        vk::HOME,
        vk::END,
        vk::RETURN,
        vk::ESCAPE,
        vk::TAB,
        vk::BACK,
        vk::INSERT,
        vk::DELETE,
        vk::SPACE,
        vk::A,
        vk::Z,
    ];
    for k in keys {
        let name = key_display_name(k);
        assert!(
            parse_key_name(&name) == Some(k),
            "round-trip failed for {name}"
        );
    }
}

#[test]
fn key_names_round_trip_with_modifiers() {
    let cases = [
        kbmod::CTRL | vk::F3,
        kbmod::ALT | vk::X,
        kbmod::SHIFT | vk::F5,
        kbmod::CTRL | vk::F4,
    ];
    for k in cases {
        let name = key_display_name(k);
        assert!(
            parse_key_name(&name) == Some(k),
            "round-trip failed for {name}"
        );
    }
}

#[test]
fn unknown_key_name_is_rejected() {
    assert!(parse_key_name("Nope").is_none());
    assert!(parse_key_name("Ctrl+Nope").is_none());
}
