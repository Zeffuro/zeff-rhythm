use std::env;
use std::error::Error;

type CommandResult = Result<(), Box<dyn Error>>;
type CommandHandler = fn(&[String]) -> CommandResult;

pub struct Command {
    name: &'static str,
    usage: &'static str,
    summary: &'static str,
    details: &'static [&'static str],
    handler: CommandHandler,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "app",
        usage: "app [--preview|--calibration] [--max-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]",
        summary: "Open the single-window winit/wgpu app shell.",
        details: &[
            "This is the default when running without arguments.",
            "Use --preview to start directly on the local chart in the same wgpu window.",
            "Use --calibration to start a generated one-lane click test; press D on each click.",
            "Use Up/Down or W/S to move, Enter/Space to open, Esc/Backspace to go back, and Q to quit.",
            "Gameplay uses D/F/J/K, CPAL audio, receipt-time winit input, core judgement, and app-session event logs.",
            "SDL play-map remains the timestamp-quality regression harness until winit/raw input is validated.",
        ],
        handler: super::app_wgpu::run,
    },
    Command {
        name: "app-sdl",
        usage: "app-sdl",
        summary: "Open the legacy SDL app scaffold.",
        details: &[
            "This shell is kept as a simple native/menu diagnostic path.",
            "The default app path is now the single-window winit/wgpu shell.",
        ],
        handler: super::app_window::run,
    },
    Command {
        name: "app-wgpu",
        usage: "app-wgpu [--preview|--calibration] [--max-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]",
        summary: "Open the single-window winit/wgpu app shell.",
        details: &[
            "Alias for the default app renderer path while the command name remains useful for diagnostics.",
            "Use --preview to start directly on the local chart in the same wgpu window.",
            "Use --calibration to start a generated one-lane click test; press D on each click.",
            "Navigation uses the same broad controls as app: arrows or W/S, Enter/Space, Esc/Backspace, Q.",
            "Gameplay uses D/F/J/K, CPAL audio, receipt-time winit input, core judgement, and app-session event logs.",
            "SDL play-map remains the timestamp-quality regression harness until winit/raw input is validated.",
        ],
        handler: super::app_wgpu::run,
    },
    Command {
        name: "smoke",
        usage: "smoke",
        summary: "Run a tiny native/core timing smoke test.",
        details: &["Creates an internal 4K chart and submits one timestamped lane input."],
        handler: super::smoke::run,
    },
    Command {
        name: "app-smoke",
        usage: "app-smoke",
        summary: "Print the initial app state/settings model without starting audio or a window.",
        details: &[
            "Exercises the menu/settings/play-launch state that will sit above the native play session.",
            "This is a structural smoke test, not the final app UI.",
        ],
        handler: super::app_smoke::run,
    },
    Command {
        name: "inspect-osu",
        usage: "inspect-osu <path-to-osu-mania-file>",
        summary: "Parse an osu!mania chart and print basic metadata.",
        details: &["Supports osu! mode 3 charts."],
        handler: super::charts::inspect_osu,
    },
    Command {
        name: "inspect-sm",
        usage: "inspect-sm <path-to-stepmania-sm-file>",
        summary: "Parse a StepMania .sm chart and print basic metadata.",
        details: &["Currently supports dance-single charts."],
        handler: super::charts::inspect_sm,
    },
    Command {
        name: "map-test",
        usage: "map-test <chart-path> [--format auto|osu|sm] [--offset-ms MS]",
        summary: "Run a deterministic non-rendered chart simulation through the core engine.",
        details: &[
            "Autoplays each note at its scheduled lane/time plus the optional offset.",
            "Use this to verify parser output and judgement behavior before the playable harness exists.",
        ],
        handler: super::map_test::run,
    },
    Command {
        name: "play-map",
        usage: "play-map <chart-path> [--format auto|osu|sm] [--audio PATH] [--input terminal|sdl] [--event-log PATH] [--input-offset-ms MS] [--max-seconds SECONDS] [--display highway|sdl|log] [--lookahead-seconds SECONDS] [--lead-in-seconds SECONDS] [--chart-start-seconds SECONDS] [--start-delay-seconds SECONDS] [--dry-run] [--host HOST] [--device NAME_OR_ID] [--sample-rate HZ] [--buffer FRAMES]",
        summary: "Play a chart's audio and judge lane input against the audio clock.",
        details: &[
            "Uses D/F/J/K for four lanes. Quit with Esc or Q.",
            "Use --input terminal for the current terminal path or --input sdl for SDL3 timestamped keyboard events.",
            "Use --event-log PATH to write input, hit, miss, and unmatched timing rows to CSV.",
            "Use --display sdl for one native SDL window that renders lanes and receives timestamped keyboard input.",
            "Default display is a terminal note highway; use --display log for line-by-line output.",
            "Use --lookahead-seconds to change how early notes appear.",
            "Use --lead-in-seconds or --chart-start-seconds to control chart time before audio starts.",
            "Use --dry-run to verify chart/audio/device setup without starting playback.",
            "Use --max-seconds for short harness checks.",
            "This is a native timing harness, not the final renderer or final input backend.",
        ],
        handler: super::play_map::run,
    },
    Command {
        name: "analyze-run",
        usage: "analyze-run <event-log.csv> [more-event-log.csv]",
        summary: "Summarize play-map CSV logs and estimate input offset adjustment.",
        details: &[
            "Reads logs written by play-map --event-log.",
            "Aggregates hit delta, absolute hit error, input queue age, ratings, misses, and unmatched inputs.",
            "Prints both negative mean and robust 10% trimmed mean input offset suggestions.",
        ],
        handler: super::run_analysis::run,
    },
    Command {
        name: "latency-probe",
        usage: "latency-probe [--host HOST] [--device NAME_OR_ID]",
        summary: "List CPAL output hosts, devices, IDs, and supported configs.",
        details: &[
            "Use --host to restrict to a backend such as WASAPI.",
            "Use --device with a name substring or stable CPAL device ID.",
        ],
        handler: super::audio_commands::latency_probe,
    },
    Command {
        name: "audio-callback-probe",
        usage: "audio-callback-probe [seconds] [--host HOST] [--device NAME_OR_ID] [--sample-rate HZ] [--buffer FRAMES]",
        summary: "Measure output callback intervals and predicted playback lead.",
        details: &[
            "Run in --release for measurements.",
            "The observed frames per callback can differ from requested --buffer.",
        ],
        handler: super::audio_commands::audio_callback_probe,
    },
    Command {
        name: "tap-probe",
        usage: "tap-probe [seconds] [bpm] [--host HOST] [--device NAME_OR_ID] [--sample-rate HZ] [--buffer FRAMES]",
        summary: "Play generated clicks and log terminal key deltas.",
        details: &[
            "Tap D, F, J, K, or Space. Quit with Esc or Q.",
            "Terminal input is provisional; SDL3 timestamps should replace it for serious input latency measurements.",
        ],
        handler: super::audio_commands::tap_probe,
    },
    Command {
        name: "sdl-input-probe",
        usage: "sdl-input-probe [seconds]",
        summary: "Open an SDL3 window and print timestamped keyboard events.",
        details: &[
            "Focus the SDL window, then press D/F/J/K to verify physical lane input timestamps.",
            "Quit with Esc, Q, or the window close button.",
            "Logs SDL nanosecond timestamps, scancode/keycode, repeat state, lane mapping, and an estimated event queue age.",
        ],
        handler: super::sdl_input_probe::run,
    },
    Command {
        name: "wgpu-smoke",
        usage: "wgpu-smoke [--frames N] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]",
        summary: "Open a winit/wgpu surface and render measured clear frames.",
        details: &[
            "Exercises the planned app-owned gameplay renderer path without audio or chart logic.",
            "Prints adapter/backend, surface format, requested/selected present mode, frame latency, and frame timing metrics.",
            "Use --present fifo for the default latency-safe path; Mailbox and Immediate are selected only when the surface reports support.",
            "This is a renderer smoke test, not the final game view.",
        ],
        handler: super::wgpu_smoke::run,
    },
    Command {
        name: "wgpu-preview",
        usage: "wgpu-preview [chart-path] [--format auto|osu|sm] [--max-seconds S] [--lookahead-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]",
        summary: "Render a real chart highway through the winit/wgpu path without audio/input.",
        details: &[
            "Defaults to the local Speedcore StepMania fixture when no chart path is supplied.",
            "Uses the shared chart parser and shared highway render-data path.",
            "This is the shared chart renderer used by the CLI and app shell; live audio/input still stay in play-map or the SDL app handoff for now.",
        ],
        handler: super::wgpu_preview::run,
    },
];

pub fn run() -> CommandResult {
    let args: Vec<String> = env::args().skip(1).collect();
    dispatch(&args)
}

fn dispatch(args: &[String]) -> CommandResult {
    let Some(name) = args.first() else {
        return super::app_wgpu::run(&[]);
    };

    if is_help(name) {
        match args.get(1) {
            Some(command_name) => print_command_help(command_name)?,
            None => print_help(),
        }
        return Ok(());
    }

    let Some(command) = find_command(name) else {
        return Err(format!("unknown command: {name}. Run `zeff-rhythm help`.").into());
    };

    if args.get(1).is_some_and(|arg| is_help(arg)) {
        print_one_command(command);
        return Ok(());
    }

    (command.handler)(&args[1..])
}

fn print_help() {
    println!("zeff-rhythm");
    println!();
    println!("Usage:");
    println!("  zeff-rhythm");
    println!("  zeff-rhythm <command> [args]");
    println!();
    println!("Running without a command opens the single-window winit/wgpu app shell.");
    println!();
    println!("Commands:");

    let width = COMMANDS
        .iter()
        .map(|command| command.name.len())
        .max()
        .unwrap_or_default();

    for command in COMMANDS {
        println!(
            "  {:width$}  {}",
            command.name,
            command.summary,
            width = width
        );
    }

    println!();
    println!("Run `zeff-rhythm help <command>` for command usage.");
}

fn print_command_help(name: &str) -> CommandResult {
    let Some(command) = find_command(name) else {
        return Err(format!("unknown command: {name}").into());
    };

    print_one_command(command);
    Ok(())
}

fn print_one_command(command: &Command) {
    println!("{}", command.name);
    println!();
    println!("Usage:");
    println!("  zeff-rhythm {}", command.usage);
    println!();
    println!("{}", command.summary);

    if !command.details.is_empty() {
        println!();
        for detail in command.details {
            println!("- {detail}");
        }
    }
}

fn find_command(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

fn is_help(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}
