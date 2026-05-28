bin := "waybar-wireguard"
lib := "libwaybar_wireguard.so"
setcap_cmd := "sudo setcap CAP_NET_ADMIN=+eip"

prefix := env("PREFIX", "/usr")
destdir := env("DESTDIR", "")

default:
    just --list

build:
    cargo build
    {{ setcap_cmd }} ./target/debug/{{ bin }}

# A user will have to do it on their system anyway.
release:
    cargo build --release
    {{ setcap_cmd }} ./target/release/{{ bin }}

run: build
    ./target/debug/{{ bin }}

check:
    cargo check

test *args:
    cargo test {{ args }}

clean:
    cargo clean

# TODO: wrap this in an Arch PKGBUILD (setcap via .install hook).
install: release
    install -Dm755 ./target/release/{{ bin }} {{ destdir }}{{ prefix }}/bin/{{ bin }}
    install -Dm755 ./target/release/{{ lib }} {{ destdir }}{{ prefix }}/lib/{{ lib }}
    install -Dm644 -t {{ destdir }}{{ prefix }}/share/waybar-wireguard/assets/ ./assets/*
    {{ setcap_cmd }} {{ destdir }}{{ prefix }}/bin/{{ bin }}

uninstall:
    rm -f  {{ destdir }}{{ prefix }}/bin/{{ bin }}
    rm -f  {{ destdir }}{{ prefix }}/lib/{{ lib }}
    rm -rf {{ destdir }}{{ prefix }}/share/waybar-wireguard
