use serde::Deserialize;
use std::cell::Cell;
use std::rc::Rc;
use waybar_cffi::{
    InitInfo, Module,
    gtk::{self, glib, prelude::*},
    waybar_module,
};

const DEFAULT_WIREGUARD_DEV: &str = "wg0";
const DEFAULT_WIREGUARD_UP_CMD_PREFIX: &str = "sudo /usr/bin/wg-quick up";
const DEFAULT_WIREGUARD_DOWN_CMD_PREFIX: &str = "sudo /usr/bin/wg-quick down";
const DEFAULT_WAYBAR_WIREGUARD_CMD: &str = "waybar-wireguard"; // Assuming it is somewhere on $PATH
const DEFAULT_REFRESH_INTERVAL_SECONDS: u8 = 5;
const DEFAULT_FORMAT: &str = "WG: {}";
const DEFAULT_LABEL_UP: &str = "✓";
const DEFAULT_LABEL_DOWN: &str = "✗";
const DEFAULT_LABEL_UNKNOWN: &str = "?";

// Default CSS #ids; override per instance via config if you run more than one.
const DEFAULT_WIDGET_NAME: &str = "cffi-wireguard";
const DEFAULT_WIDGET_ICON_NAME: &str = "cffi-wireguard-icon";

// ---------- types ----------

#[derive(Deserialize, Debug, PartialEq)]
#[serde(default)]
struct Config {
    helper_cmd: String,
    wireguard_dev: String,
    wireguard_up_cmd_prefix: String,
    wireguard_down_cmd_prefix: String,
    refresh_interval_seconds: u8,
    #[serde(deserialize_with = "deserialize_format")]
    format: String,
    label_up: String,
    label_down: String,
    label_unknown: String,
    widget_name: String,
    widget_icon_name: String,
}

/// Accept either a string (used as-is) or a map (waybar rewrites the literal
/// `"{}"` into an empty object -- in that case fall back to the default format).
fn deserialize_format<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrMap {
        Str(String),
        Map(serde::de::IgnoredAny),
    }
    Ok(match StringOrMap::deserialize(deserializer)? {
        StringOrMap::Str(s) => s,
        StringOrMap::Map(_) => DEFAULT_FORMAT.into(),
    })
}

impl Config {
    fn cmd(&self, prefix: &str) -> Vec<String> {
        prefix
            .split_whitespace()
            .chain(std::iter::once(self.wireguard_dev.as_str()))
            .map(str::to_owned)
            .collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            helper_cmd: DEFAULT_WAYBAR_WIREGUARD_CMD.into(),
            wireguard_dev: DEFAULT_WIREGUARD_DEV.into(),
            wireguard_up_cmd_prefix: DEFAULT_WIREGUARD_UP_CMD_PREFIX.into(),
            wireguard_down_cmd_prefix: DEFAULT_WIREGUARD_DOWN_CMD_PREFIX.into(),
            refresh_interval_seconds: DEFAULT_REFRESH_INTERVAL_SECONDS,
            format: DEFAULT_FORMAT.into(),
            label_up: DEFAULT_LABEL_UP.into(),
            label_down: DEFAULT_LABEL_DOWN.into(),
            label_unknown: DEFAULT_LABEL_UNKNOWN.into(),
            widget_name: DEFAULT_WIDGET_NAME.into(),
            widget_icon_name: DEFAULT_WIDGET_ICON_NAME.into(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum WgState {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, PartialEq)]
struct WgDump {
    state: WgState,
    tooltip: String,
}

#[derive(Debug, PartialEq)]
struct Render {
    label: String,
    tooltip: String,
    state: WgState,
    active: bool,
}

struct WgModule {
    _label: gtk::Label,
    _horizontal_box: gtk::Box,
    _event_box: gtk::EventBox,
}

// ---------- pure helpers ----------

fn parse_config(raw: serde_jsonc::Value) -> Config {
    serde_jsonc::from_value(raw.clone()).unwrap_or_else(|e| {
        eprintln!(
            "waybar-wireguard: failed to parse module config: {e}\n\
             waybar-wireguard:   received: {raw:#}\n\
             waybar-wireguard:   falling back to defaults; check your waybar config \
             for a key with the wrong value type."
        );
        Config::default()
    })
}

fn apply_format(fmt: &str, status: &str) -> String {
    fmt.replace("{}", status)
}

fn parse_wg_dump(bytes: &[u8]) -> WgDump {
    let value: serde_jsonc::Value = serde_jsonc::from_slice(bytes).unwrap_or_else(|e| {
        eprintln!("waybar-wireguard: helper output isn't JSON: {e}");
        serde_jsonc::Value::Null
    });

    let state = match value.get("state").and_then(|v| v.as_str()) {
        Some("up") => WgState::Up,
        Some("down") => WgState::Down,
        _ => WgState::Unknown,
    };

    // Render every {string-keyed string-valued} field as "key: value" per line.
    let tooltip = value
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}: {s}")))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    WgDump { state, tooltip }
}

/// Decide what the UI should show from the helper's exit status + output.
/// Pure: no GTK, no spawning -- easy to feed synthetic `Output` values to.
fn render_from_helper(
    config: &Config,
    result: Result<std::process::Output, std::io::Error>,
) -> Render {
    let cmd = &config.helper_cmd;
    match result {
        Ok(out) if out.status.success() => {
            let dump = parse_wg_dump(&out.stdout);
            let glyph = match dump.state {
                WgState::Up => &config.label_up,
                WgState::Down => &config.label_down,
                WgState::Unknown => &config.label_unknown,
            };
            Render {
                label: apply_format(&config.format, glyph),
                tooltip: dump.tooltip,
                state: dump.state,
                active: matches!(dump.state, WgState::Up),
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!(
                "waybar-wireguard: `{cmd}` exited with {}: {stderr}",
                out.status
            );
            Render {
                label: apply_format(&config.format, &config.label_unknown),
                tooltip: format!("`{cmd}` exited with {}\n{stderr}", out.status),
                state: WgState::Unknown,
                active: false,
            }
        }
        Err(e) => {
            eprintln!("waybar-wireguard: failed to spawn `{cmd}`: {e}");
            Render {
                label: apply_format(&config.format, &config.label_unknown),
                tooltip: format!("failed to spawn `{cmd}`: {e}"),
                state: WgState::Unknown,
                active: false,
            }
        }
    }
}

// ---------- GTK glue ----------

fn build_widgets(
    container: &gtk::Container,
    config: &Config,
) -> (gtk::Label, gtk::Box, gtk::EventBox) {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_widget_name(&config.widget_name);

    let icon = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    icon.set_widget_name(&config.widget_icon_name);
    hbox.add(&icon);

    let label = gtk::Label::new(Some("?"));
    hbox.add(&label);

    hbox.set_has_tooltip(true);
    hbox.set_tooltip_text(Some("loading…"));

    let ebox = gtk::EventBox::new();
    ebox.add(&hbox);
    container.add(&ebox);
    container.show_all();

    (label, hbox, ebox)
}

fn set_state(widget: &gtk::Box, state: WgState) {
    let ctx = widget.style_context();
    match state {
        WgState::Up => {
            ctx.remove_class("inactive");
            ctx.remove_class("unknown");
        }
        WgState::Down => {
            ctx.add_class("inactive");
            ctx.remove_class("unknown");
        }
        WgState::Unknown => {
            ctx.remove_class("inactive");
            ctx.add_class("unknown");
        }
    }
}

fn spawn_refresh(
    label: &gtk::Label,
    hbox: &gtk::Box,
    active: &Rc<Cell<bool>>,
    config: &Rc<Config>,
) {
    let label = label.clone();
    let hbox = hbox.clone();
    let active = Rc::clone(active);
    let config = Rc::clone(config);
    glib::MainContext::default().spawn_local(async move {
        let result = async_process::Command::new(&config.helper_cmd)
            .arg(&config.wireguard_dev)
            .output()
            .await;
        let render = render_from_helper(&config, result);
        label.set_text(&render.label);
        hbox.set_tooltip_text(Some(&render.tooltip));
        active.set(render.active);
        set_state(&hbox, render.state);
    });
}

fn spawn_toggle(
    label: &gtk::Label,
    hbox: &gtk::Box,
    active: &Rc<Cell<bool>>,
    config: &Rc<Config>,
    argv: Vec<String>,
) {
    let label = label.clone();
    let hbox = hbox.clone();
    let active = Rc::clone(active);
    let config = Rc::clone(config);
    glib::MainContext::default().spawn_local(async move {
        let pretty = argv.join(" ");
        match async_process::Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                spawn_refresh(&label, &hbox, &active, &config);
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!(
                    "waybar-wireguard: `{pretty}` exited with {}: {stderr}",
                    out.status
                );
            }
            Err(e) => {
                eprintln!("waybar-wireguard: failed to spawn `{pretty}`: {e}");
            }
        }
    });
}

// ---------- Module impl ----------

impl Module for WgModule {
    type Config = serde_jsonc::Value;

    fn init(info: &InitInfo, raw: serde_jsonc::Value) -> Self {
        let config = Rc::new(parse_config(raw));
        let active = Rc::new(Cell::new(false));
        let (label, hbox, ebox) = build_widgets(&info.get_root_widget(), &config);

        spawn_refresh(&label, &hbox, &active, &config);

        let label_weak = label.downgrade();
        let hbox_weak = hbox.downgrade();
        let timer_active = Rc::clone(&active);
        let timer_config = Rc::clone(&config);
        glib::timeout_add_seconds_local(config.refresh_interval_seconds as u32, move || {
            let (Some(label), Some(hbox)) = (label_weak.upgrade(), hbox_weak.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            spawn_refresh(&label, &hbox, &timer_active, &timer_config);
            glib::ControlFlow::Continue
        });

        let click_label = label.clone();
        let click_hbox = hbox.clone();
        let click_active = Rc::clone(&active);
        let click_config = Rc::clone(&config);
        ebox.connect_button_press_event(move |_, _ev| {
            let argv = if click_active.get() {
                click_config.cmd(&click_config.wireguard_down_cmd_prefix)
            } else {
                click_config.cmd(&click_config.wireguard_up_cmd_prefix)
            };
            spawn_toggle(
                &click_label,
                &click_hbox,
                &click_active,
                &click_config,
                argv,
            );
            glib::Propagation::Stop
        });

        Self {
            _label: label,
            _horizontal_box: hbox,
            _event_box: ebox,
        }
    }
}
waybar_module!(WgModule);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::empty_format("", "X", "")]
    #[case::empty_label("WG: {}", "", "WG: ")]
    #[case::no_placeholder("plain", "X", "plain")]
    #[case::ascii_only("[WG: {}]", "X", "[WG: X]")]
    #[case::unicode("🛡️: {}", "працює!", "🛡️: працює!")]
    #[case::replaces_all("{} {}", "X", "X X")]
    #[case::single_pass_not_recursive("{}", "{}", "{}")]
    #[case::only_exact_braces_match("{a} {} {b}", "X", "{a} X {b}")]
    fn apply_format_cases(#[case] fmt: &str, #[case] status: &str, #[case] expected: &str) {
        assert_eq!(apply_format(fmt, status), expected);
    }

    #[rstest]
    #[case::default_up_prefix(
        "sudo /usr/bin/wg-quick up",
        "wg0",
        &["sudo", "/usr/bin/wg-quick", "up", "wg0"],
    )]
    #[case::single_word_prefix("your_cmd", "wg0", &["your_cmd", "wg0"])]
    #[case::collapses_multiple_spaces(
        "sudo  wg-quick   up",
        "wg0",
        &["sudo", "wg-quick", "up", "wg0"],
    )]
    #[case::trims_outer_whitespace(
        "  sudo wg-quick up  ",
        "wg0",
        &["sudo", "wg-quick", "up", "wg0"],
    )]
    #[case::tab_separator("sudo\twg-quick\tup", "wg0", &["sudo", "wg-quick", "up", "wg0"])]
    #[case::empty_prefix_only_dev("", "wg0", &["wg0"])]
    #[case::whitespace_only_prefix("   ", "wg0", &["wg0"])]
    #[case::non_default_dev("wg-quick up", "vpn0", &["wg-quick", "up", "vpn0"])]
    fn config_cmd_cases(#[case] prefix: &str, #[case] dev: &str, #[case] expected: &[&str]) {
        let config = Config {
            wireguard_dev: dev.into(),
            ..Config::default()
        };
        let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(config.cmd(prefix), expected);
    }

    #[rstest]
    #[case::empty("", WgState::Unknown, "")]
    #[case::minimal("{}", WgState::Unknown, "")]
    #[case::invalid_json("not json", WgState::Unknown, "")]
    #[case::null_top("null", WgState::Unknown, "")]
    #[case::array_top("[]", WgState::Unknown, "")]
    #[case::state_up(r#"{"state":"up"}"#, WgState::Up, "state: up")]
    #[case::state_down(r#"{"state":"down"}"#, WgState::Down, "state: down")]
    #[case::unrecognised_state_value(r#"{"state":"weird"}"#, WgState::Unknown, "state: weird")]
    #[case::state_non_string_falls_back(r#"{"state":42}"#, WgState::Unknown, "")]
    #[case::filters_non_string_values(r#"{"state":"up","count":3}"#, WgState::Up, "state: up")]
    #[case::realistic(
        r#"{"state":"up","interface":"wg0","latest handshake":"5 seconds ago"}"#,
        WgState::Up,
        "interface: wg0\nlatest handshake: 5 seconds ago\nstate: up"
    )]
    fn parse_wg_dump_cases(#[case] input: &str, #[case] state: WgState, #[case] tooltip: &str) {
        let wg_dump = WgDump {
            state,
            tooltip: tooltip.into(),
        };
        assert_eq!(parse_wg_dump(input.as_bytes()), wg_dump)
    }

    #[rstest]
    #[case::empty_map_uses_defaults("{}", Config::default())]
    #[case::partial_override_keeps_other_defaults(
        r#"{"helper_cmd":"/usr/local/bin/wg-status","wireguard_dev":"vpn0"}"#,
        Config {
            helper_cmd: "/usr/local/bin/wg-status".into(),
            wireguard_dev: "vpn0".into(),
            ..Config::default()
        },
    )]
    #[case::format_string_preserved(
        r#"{"format":"VPN: {}"}"#,
        Config { format: "VPN: {}".into(), ..Config::default() },
    )]
    #[case::format_empty_map_falls_back(
        r#"{"format":{},"helper_cmd":"x"}"#,
        Config { helper_cmd: "x".into(), ..Config::default() },
    )]
    #[case::unknown_field_silently_ignored(r#"{"foo": "/etc/foo"}"#, Config::default())]
    // this is a base case actually, as the module_path is passed to the pulgin too
    #[case::module_path_is_ignored_by_the_config_parser(
        r#"{"module_path": "/foo/bar.so", "format": "VPN: {}", "helper_cmd":"x"}"#,
        Config { helper_cmd: "x".into(), format: "VPN: {}".into(), ..Config::default() },
    )]
    #[case::bad_field_type_falls_back_to_full_default(
        r#"{"helper_cmd":42,"wireguard_dev":"would-have-been-kept"}"#,
        Config::default()
    )]
    fn parse_config_cases(#[case] input: &str, #[case] expected: Config) {
        let raw: serde_jsonc::Value = serde_jsonc::from_str(input).unwrap();
        assert_eq!(parse_config(raw), expected);
    }

    use std::io;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    fn ok_out(stdout: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn nonzero_out(exit_code: i32, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(exit_code << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[rstest]
    #[case::success_up(
        Ok(ok_out(r#"{"state":"up"}"#)),
        Render {
            label: "WG: ✓".into(),
            tooltip: "state: up".into(),
            state: WgState::Up,
            active: true,
        },
    )]
    #[case::success_down(
        Ok(ok_out(r#"{"state":"down"}"#)),
        Render {
            label: "WG: ✗".into(),
            tooltip: "state: down".into(),
            state: WgState::Down,
            active: false,
        },
    )]
    #[case::success_unrecognised_state(
        Ok(ok_out(r#"{"state":"weird"}"#)),
        Render {
            label: "WG: ?".into(),
            tooltip: "state: weird".into(),
            state: WgState::Unknown,
            active: false,
        },
    )]
    #[case::success_with_extra_fields(
        Ok(ok_out(r#"{"state":"up","interface":"wg0"}"#)),
        Render {
            label: "WG: ✓".into(),
            tooltip: "interface: wg0\nstate: up".into(),
            state: WgState::Up,
            active: true,
        },
    )]
    #[case::non_zero_exit(
        Ok(nonzero_out(1, "boom")),
        Render {
            label: "WG: ?".into(),
            tooltip: "`waybar-wireguard` exited with exit status: 1\nboom".into(),
            state: WgState::Unknown,
            active: false,
        },
    )]
    #[case::spawn_error(
        Err(io::Error::new(io::ErrorKind::NotFound, "no helper")),
        Render {
            label: "WG: ?".into(),
            tooltip: "failed to spawn `waybar-wireguard`: no helper".into(),
            state: WgState::Unknown,
            active: false,
        },
    )]
    fn render_from_helper_cases(
        #[case] result: Result<Output, io::Error>,
        #[case] expected: Render,
    ) {
        let config = Config::default();
        assert_eq!(render_from_helper(&config, result), expected);
    }
}
