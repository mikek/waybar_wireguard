# waybar-wireguard

A [Waybar][1] CFFI module (built on [`waybar-cffi`][2]) that shows the state and stats
of a [WireGuard][3] interface and lets you turn it on and off.

See in action:
<video src="https://github.com/user-attachments/assets/398c5fcd-9d91-4fa6-b870-e8f49cf64955" autoplay muted loop></video>

Features:

- Doesn't depend on a network manager — just the standard WireGuard tools and kernel interface.
- Showing the tunnel state needs no root or sudo — only `CAP_NET_ADMIN` on the helper; sudo is only invoked for whatever up/down command you configure.
- Multiple instances of the plugin are supported.
- Full customization via config and CSS (custom labels, classes, icons).

**At the moment only Linux kernel-mode interfaces are supported.**

## Installation

### On Arch Linux

Available on the [AUR][7] as [`waybar-wireguard`][7]:

```sh
paru -S waybar-wireguard      # or: yay -S waybar-wireguard
```

`CAP_NET_ADMIN` is granted to the helper automatically via the package's
post-install hook.

### Manually

If there is no package for your distribution (and most likely there isn't one), you'll have to build it. This assumes you have a working Rust toolchain (at least `cargo` & `rustc`).

```sh
git clone https://github.com/mikek/waybar_wireguard
cd waybar_wireguard
cargo build --release
sudo setcap CAP_NET_ADMIN=+eip ./target/release/waybar-wireguard
install -Dm755 ./target/release/libwaybar_wireguard.so ~/tmp/  # replace with your destination
install -Dm755 ./target/release/waybar-wireguard ~/tmp/  # replace with your destination
```

You can put these files anywhere — just point Waybar at the right paths.

### A distribution-agnostic system-wide installation

(This is easier with `just`; if you don't have it, the recipes in `justfile` are short enough to run by hand.)

Run `just install` (optionally overriding the defaults):

```sh
just install                    # by default installs to /usr (needs sudo for setcap + fs write)
PREFIX=/usr/local just install  # with a custom prefix
DESTDIR=/tmp/pkg just install   # for packagers
```

Installed files:

- `$PREFIX/bin/waybar-wireguard` — helper (gets `CAP_NET_ADMIN` via `setcap`)
- `$PREFIX/lib/libwaybar_wireguard.so` — the CFFI module
- `$PREFIX/share/waybar-wireguard/assets/shield-*.svg` — generic icons, if you want any in your CSS.
- `$PREFIX/share/waybar-wireguard/assets/LICENSE` — Lucide license

## Waybar config

Just add a reference to the `cffi` module and point it at the `.so`:

```jsonc
"modules-right": ["cffi/wg"],

"cffi/wg": {
    "module_path": "/usr/lib/libwaybar_wireguard.so",

    // Every setting below is optional if you need only wg0 & the helper is on your $PATH.
    // These are the default values.

    "helper_cmd":                "waybar-wireguard",      // on $PATH or absolute
    "wireguard_dev":             "wg0",
    "wireguard_up_cmd_prefix":   "sudo /usr/bin/wg-quick up",
    "wireguard_down_cmd_prefix": "sudo /usr/bin/wg-quick down",
    "refresh_interval_seconds":  5,

    "format":        "WG: {}",     // {} is replaced with one of label_* states below
    "label_up":      "✓",
    "label_down":    "✗",
    "label_unknown": "?",

    // Can be useful for css styling:
    "widget_name":      "cffi-wireguard",
    "widget_icon_name": "cffi-wireguard-icon"
}
```

### `format` and `label_*`

- `format` is a plain template. The literal `{}` is substituted with one of
  `label_up`, `label_down`, or `label_unknown` depending on the current state.
- `label_*` and `format` are **plain text only** — no Pango markup, no HTML.
  Whatever you write is shown verbatim. Unicode is fine (the defaults are these
  characters: `✓`/`✗`/`?`).
- An empty `format` (or one that produces empty text) is valid; in that case
  only the icon is shown if you've provided it via CSS. Hover still works (the
  tooltip is attached to the whole module, not the text).

## Mouse click behavior

Clicking the module spawns `wireguard_up_cmd_prefix <dev>` if the interface is
not currently up, or `wireguard_down_cmd_prefix <dev>` if it is. Output and
errors go to waybar's stderr (`journalctl --user -u waybar` or wherever Waybar
is logging).

Since the up/down commands typically need root, set up a NOPASSWD sudoers rule
scoped to the exact commands you configured.

## Styling

The module's widget tree:

```
EventBox
└─ hbox#<widget_name>           (default: cffi-wireguard)
   ├─ box#<widget_icon_name>    (default: cffi-wireguard-icon)
   └─ label (the text)
```

The hbox carries one of these state classes, swapped on every refresh:

- (none)      — interface is up
- `.inactive` — interface is down
- `.unknown`  — helper failed or interface state can't be determined

Example `style.css`:

```css
#cffi-wireguard.inactive {
    color: #777;
}
#cffi-wireguard-icon {
    background-image: url("/usr/share/waybar-wireguard/assets/shield-check-white.svg");
    background-repeat: no-repeat;
    background-position: center;
    background-size: contain;
    min-width: 16px;
    min-height: 16px;
}
#cffi-wireguard.inactive #cffi-wireguard-icon {
    opacity: 0.4;
}
#cffi-wireguard.unknown #cffi-wireguard-icon {
    opacity: 0.5;
}
```

### Changing the icon

`just install` drops a small generic shield SVG icon set at
`/usr/share/waybar-wireguard/assets/` (from [Lucide][4], ISC-licensed — see the sibling `LICENSE`). The CSS example above already points there.

If you want the **official WireGuard logo** for your personal setup, the
glyph is drawn at [Simple Icons][5] under
CC0:

```sh
curl -o ~/.config/waybar/wireguard.svg \
  https://cdn.simpleicons.org/wireguard
```

CC0 only covers the SVG file — the mark itself is a trademark. WireGuard's
[trademark policy][6] asks you to
email them before using it with third-party software, though approval is
reportedly routine for personal/community use.

### Multiple instances

If you are not using icons in your CSS and do not need different styles, setting
label formats like `"format": "WG1: {}"` & `"format": "WG2: {}"` is enough.

However, Waybar doesn't set distinct widget names on the parent containers of CFFI
modules. So if you run more than one instance of this module you might want to give
each one its own `widget_name` and `widget_icon_name` (though you don't have to):

```jsonc
"cffi/wg-personal": {
    "module_path":      "/usr/lib/libwaybar_wireguard.so",
    "wireguard_dev":    "wg0",
    "widget_name":      "wg-personal",
    "widget_icon_name": "wg-personal-icon"
},
"cffi/wg-work": {
    "module_path":      "/usr/lib/libwaybar_wireguard.so",
    "wireguard_dev":    "wg1",
    "widget_name":      "wg-work",
    "widget_icon_name": "wg-work-icon"
}
```

Then style them independently:

```css
#wg-personal.inactive { color: #889; }
#wg-work.inactive     { color: #988; }
```

---

# Development

## Components

- **`libwaybar_wireguard.so`** — the CFFI module that Waybar loads.
- **`waybar-wireguard`** — a small helper binary that queries the kernel for
  WireGuard state via netlink (needs `CAP_NET_ADMIN`).

## Testing

Make sure to add a reasonable set of tests. Testing kernel mode interface operations and GTK
might not be worth it in this case.

```sh
just test
```

## Build

```sh
just build       # debug: cargo build + setcap on the helper
just release     # release: cargo build --release + setcap
```

## Note on `"format": "{}"`

Waybar rewrites the literal string `"{}"` into an empty JSON object before
handing config to CFFI modules. The deserializer detects that case and falls
back to the default format (`"{}"`), so users can keep writing
`"format": "{}"` in their config without seeing a parse error.

[1]: https://github.com/Alexays/Waybar
[2]: https://crates.io/crates/waybar-cffi
[3]: https://www.wireguard.com/
[4]: https://lucide.dev
[5]: https://simpleicons.org/?q=wireguard
[6]: https://www.wireguard.com/trademark-policy/
[7]: https://aur.archlinux.org/packages/waybar-wireguard
