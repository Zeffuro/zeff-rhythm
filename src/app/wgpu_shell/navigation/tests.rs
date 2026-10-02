use super::*;
use std::fs;

#[test]
fn control_a_replaces_or_deletes_search_without_adding_a() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.search_active = true;
    shell.window_focused = true;
    shell.search_query = "old search".into();
    shell.modifiers = ModifiersState::CONTROL;
    assert!(shell.search_input(KeyCode::KeyA, Some("a"), true));
    assert!(shell.search_selected_all);
    assert_eq!(shell.search_query, "old search");
    shell.modifiers = ModifiersState::empty();
    shell.insert_search_text("日本語");
    assert_eq!(shell.search_query, "日本語");
    shell.modifiers = ModifiersState::CONTROL;
    shell.search_input(KeyCode::KeyA, Some("a"), true);
    shell.modifiers = ModifiersState::empty();
    shell.search_input(KeyCode::Backspace, None, true);
    assert!(shell.search_query.is_empty());
}

#[test]
fn search_backspace_removes_one_grapheme_and_commands_pass_through() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.search_active = true;
    shell.window_focused = true;
    shell.insert_search_text("cafe\u{301}");
    shell.search_input(KeyCode::Backspace, None, true);
    assert_eq!(shell.search_query, "caf");
    for code in [KeyCode::F1, KeyCode::F2, KeyCode::F5, KeyCode::ArrowDown] {
        assert!(!shell.search_input(code, None, true));
    }
    shell.help_visible = true;
    shell.insert_search_text("ignored");
    assert_eq!(shell.search_query, "caf");
}

#[test]
fn held_navigation_and_random_respect_available_search_results() {
    let root = std::env::temp_dir().join(format!("zeff-navigation-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("song.wav"), b"fixture").unwrap();
    for (name, lanes) in [("A", 4), ("B", 4), ("C", 4), ("Unavailable", 7)] {
        fs::write(root.join(format!("{name}.osu")), format!(
            "[General]\nMode:3\nAudioFilename:song.wav\n[Metadata]\nTitle:Match {name}\nTitleUnicode:日本語 {name}\nArtist:Artist\n[Difficulty]\nCircleSize:{lanes}\n[HitObjects]\n64,192,1000,1,0\n"
        )).unwrap();
    }
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.library = AppLibrary::scan_roots_with_cache(&[root.clone()], &root.join("cache"));
    shell.library_index = 0;
    assert!(shell.handle_library_key(KeyCode::ArrowDown, true, false));
    assert_eq!(shell.library_index, 1);
    assert!(shell.handle_library_key(KeyCode::ArrowDown, true, true));
    assert_eq!(shell.library_index, 2);
    shell.handle_library_key(KeyCode::ArrowUp, true, true);
    assert_eq!(shell.library_index, 1);
    assert!(!shell.handle_library_key(KeyCode::ArrowDown, false, false));
    shell.ready_only = false;
    shell.search_query = "Match".into();
    assert_eq!(shell.library_indices().len(), 4);
    for _ in 0..20 {
        let old = shell.library_index;
        shell.random_song();
        assert_ne!(old, shell.library_index);
        assert!(
            shell
                .library
                .get(shell.library_index)
                .unwrap()
                .is_available()
        );
    }
    shell.search_query = "日本語 B".into();
    shell.random_song();
    assert_eq!(
        shell.library.get(shell.library_index).unwrap().title,
        "日本語 B"
    );
    let old = shell.library_index;
    shell.handle_library_key(KeyCode::F2, true, true);
    assert_eq!(old, shell.library_index);
    fs::remove_dir_all(root).unwrap();
}
