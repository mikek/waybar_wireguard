bin := "waybar-wireguard"
lib := "libwaybar_wireguard.so"
setcap_cmd := "sudo setcap CAP_NET_ADMIN=+eip"

prefix := env("PREFIX", "/usr")
destdir := env("DESTDIR", "")

aur_dir := env("AUR_DIR", "../waybar-wireguard-arch")

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
    rm -rf packaging/arch/pkg packaging/arch/src
    rm -f  packaging/arch/*.log packaging/arch/*.tar.zst packaging/arch/*.tar.gz

install: release
    install -Dm755 ./target/release/{{ bin }} {{ destdir }}{{ prefix }}/bin/{{ bin }}
    install -Dm755 ./target/release/{{ lib }} {{ destdir }}{{ prefix }}/lib/{{ lib }}
    install -Dm644 -t {{ destdir }}{{ prefix }}/share/waybar-wireguard/assets/ ./assets/*
    {{ setcap_cmd }} {{ destdir }}{{ prefix }}/bin/{{ bin }}

uninstall:
    rm -f  {{ destdir }}{{ prefix }}/bin/{{ bin }}
    rm -f  {{ destdir }}{{ prefix }}/lib/{{ lib }}
    rm -rf {{ destdir }}{{ prefix }}/share/waybar-wireguard

# Refresh .SRCINFO and copy PKGBUILD/.SRCINFO/.install into the AUR clone (you commit & push there yourself).
aur-sync:
    @test -d {{ aur_dir }} || { echo "Error: {{ aur_dir }} not found (set AUR_DIR= to override)"; exit 1; }
    cd packaging/arch && makepkg --printsrcinfo > .SRCINFO
    cp packaging/arch/PKGBUILD packaging/arch/.SRCINFO packaging/arch/waybar-wireguard.install {{ aur_dir }}/
    @echo "Synced to {{ aur_dir }}. Review with: git -C {{ aur_dir }} diff"
