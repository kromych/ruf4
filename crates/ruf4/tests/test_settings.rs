// Copyright (c) 2026 ruf4 contributors.
// Licensed under the MIT License.

//! Settings persistence round trips. The settings location comes from the
//! environment, so this file holds a single test: tests in one binary run in
//! parallel and would race on it.

use std::path::PathBuf;

use ruf4::action::{self, Action};
use ruf4::panel::{Panel, make_entry};
use ruf4::settings::{self, Settings};
use ruf4::state::State;
use ruf4_tui::input::vk;

fn keys_of(bindings: &[action::Binding], action: Action) -> Vec<String> {
    bindings
        .iter()
        .filter(|b| b.action == action)
        .map(|b| action::key_display_name(b.key))
        .collect()
}

#[test]
fn test_bindings_survive_save_and_load() {
    let dir = std::env::temp_dir().join(format!("ruf4_settings_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: nothing else in this test binary reads the environment.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("APPDATA", &dir);
    }

    let panel = || Panel::with_entries(PathBuf::from("/test"), vec![make_entry("..", true, 0)]);
    let mut state = State::for_testing(panel(), panel());
    // Delete loses its second key (the default is F8 and Del), Quit loses all.
    state
        .bindings
        .retain(|b| !(b.action == Action::Delete && b.key == vk::DELETE));
    state.bindings.retain(|b| b.action != Action::Quit);
    assert_eq!(keys_of(&state.bindings, Action::Delete), ["F8"]);

    settings::settings_from_state(&state).save().unwrap();
    // Saved twice, as happens on every quit: what was loaded must save the same.
    let loaded = Settings::load().expect("settings load");
    loaded.save().unwrap();
    let loaded = Settings::load().expect("settings load");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(keys_of(&loaded.bindings, Action::Delete), ["F8"]);
    assert!(keys_of(&loaded.bindings, Action::Quit).is_empty());
    assert_eq!(
        keys_of(&loaded.bindings, Action::Move),
        keys_of(&action::default_bindings(), Action::Move)
    );
}
