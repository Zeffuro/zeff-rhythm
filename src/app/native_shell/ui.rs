use super::*;

pub(super) fn draw_header(
    canvas: &mut Canvas<Window>,
    width: u32,
    section: u8,
) -> Result<(), Box<dyn Error>> {
    canvas.set_draw_color(Color::RGB(29, 34, 43));
    canvas.fill_rect(Rect::new(0, 0, width, 72))?;
    canvas.set_draw_color(section_color(section));
    canvas.fill_rect(Rect::new(40, 26, width / 3, 12))?;
    canvas.fill_rect(Rect::new(40, 46, width / 5, 8))?;
    draw_text(canvas, 40, 22, "ZEFF RHYTHM", 3, TEXT)?;
    draw_text(canvas, 40, 52, screen_header(section), 2, MUTED_TEXT)?;
    Ok(())
}

pub(super) fn draw_footer(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    input_offset_ms: f64,
) -> Result<(), Box<dyn Error>> {
    canvas.set_draw_color(Color::RGB(29, 34, 43));
    canvas.fill_rect(Rect::new(0, height as i32 - 64, width, 64))?;
    canvas.set_draw_color(Color::RGB(76, 143, 191));
    let offset_width = (input_offset_ms.abs().round() as u32).clamp(4, width / 3);
    canvas.fill_rect(Rect::new(40, height as i32 - 38, offset_width, 10))?;
    draw_text(
        canvas,
        40,
        height as i32 - 52,
        &format!("INPUT OFFSET {:.3} MS", input_offset_ms),
        2,
        MUTED_TEXT,
    )?;
    Ok(())
}

pub(super) fn draw_menu_row(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    selected: bool,
    accent: Color,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    let background = if selected {
        Color::RGB(54, 61, 73)
    } else {
        Color::RGB(33, 39, 48)
    };
    canvas.set_draw_color(background);
    canvas.fill_rect(Rect::new(x, y, width, height))?;
    canvas.set_draw_color(accent);
    canvas.fill_rect(Rect::new(x, y, 10, height))?;
    let inner_width = width.saturating_sub(60);
    canvas.fill_rect(Rect::new(
        x + 34,
        y + height as i32 / 2 - 6,
        inner_width,
        12,
    ))?;
    let scale = if height >= 70 { 3 } else { 2 };
    draw_text(
        canvas,
        x + 34,
        y + (height as i32 - text_height(scale) as i32) / 2,
        label,
        scale,
        TEXT,
    )?;
    if selected {
        canvas.set_draw_color(Color::RGB(232, 228, 212));
        canvas.draw_rect(Rect::new(x - 6, y - 6, width + 12, height + 12))?;
    }
    Ok(())
}

pub(super) fn screen_label(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "main_menu",
        AppScreen::SongSelect => "song_select",
        AppScreen::Settings(SettingsPanel::Audio) => "settings.audio",
        AppScreen::Settings(SettingsPanel::Input) => "settings.input",
        AppScreen::Settings(SettingsPanel::Video) => "settings.video",
        AppScreen::Settings(SettingsPanel::Gameplay) => "settings.gameplay",
        AppScreen::Settings(SettingsPanel::Diagnostics) => "settings.diagnostics",
        AppScreen::Calibration => "calibration",
        AppScreen::Diagnostics => "diagnostics",
        AppScreen::Gameplay => "gameplay",
        AppScreen::Results => "results",
    }
}

pub(super) fn selected_label(shell: &NativeAppShell) -> String {
    match shell.state.screen {
        AppScreen::MainMenu => MENU_ITEMS[shell.menu_index].label().to_owned(),
        AppScreen::Settings(panel) => settings_selected_label(panel, shell.settings_row_index),
        AppScreen::SongSelect => shell
            .library
            .get(shell.library_index)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "none".to_owned()),
        AppScreen::Calibration => "generated_test_pending".to_owned(),
        AppScreen::Diagnostics => "runtime_summary".to_owned(),
        AppScreen::Gameplay => {
            if shell.pending_session_options.is_some() {
                shell
                    .pending_session_summary
                    .as_ref()
                    .map(|summary| summary.title.clone())
                    .unwrap_or_else(|| "play_session_ready".to_owned())
            } else {
                "play_session_pending".to_owned()
            }
        }
        AppScreen::Results => "latest_run_pending".to_owned(),
    }
}

fn screen_header(section: u8) -> &'static str {
    match section {
        1 => "SETTINGS",
        2 => "SONG SELECT",
        3 => "CALIBRATION",
        4 => "GAMEPLAY",
        5 => "RESULTS",
        _ => "MAIN MENU",
    }
}

fn section_color(section: u8) -> Color {
    match section {
        1 => Color::RGB(112, 194, 145),
        2 => Color::RGB(90, 166, 221),
        3 => Color::RGB(238, 181, 81),
        4 => Color::RGB(126, 206, 170),
        5 => Color::RGB(178, 132, 214),
        _ => Color::RGB(96, 151, 213),
    }
}

pub(super) fn panel_color(panel: SettingsPanel) -> Color {
    match panel {
        SettingsPanel::Audio => Color::RGB(90, 166, 221),
        SettingsPanel::Input => Color::RGB(112, 194, 145),
        SettingsPanel::Video => Color::RGB(238, 181, 81),
        SettingsPanel::Gameplay => Color::RGB(178, 132, 214),
        SettingsPanel::Diagnostics => Color::RGB(210, 112, 128),
    }
}
