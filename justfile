name := 'cosmic-sticky'
appid := 'io.github.Allan_J0hn.CosmicSticky'

# Installs to ~/.local by default; override with PREFIX=/usr (and rootdir= for packaging).
rootdir := ''
prefix := env('PREFIX', home_directory() / '.local')
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share' / 'applications' / appid + '.desktop'
icon-dst := base-dir / 'share' / 'icons' / 'hicolor' / 'scalable' / 'apps' / appid + '.svg'
# The path the installed binary runs from (without rootdir), written into the .desktop files.
bin-exec := absolute_path(clean(prefix)) / 'bin' / name
config-home := env('XDG_CONFIG_HOME', home_directory() / '.config')
autostart-dst := clean(rootdir / config-home) / 'autostart' / appid + '.desktop'

default: build-release

build-debug *args:
    cargo build --locked {{args}}

build-release *args: (build-debug '--release' args)

check *args:
    cargo clippy --all-targets --locked {{args}} -- -D warnings

test *args:
    cargo test --locked {{args}}

run *args:
    env RUST_LOG=${RUST_LOG:-cosmic_sticky=debug} cargo run --release --locked -- {{args}}

clean:
    cargo clean

# Builds, installs, and starts the notes at login. Notes in ~/.local/share/cosmic-sticky are untouched.
install: build-release
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{bin-dst}}
    install -Dm0644 resources/icons/{{appid}}.svg {{icon-dst}}
    install -d {{ parent_directory(desktop-dst) }} {{ parent_directory(autostart-dst) }}
    sed 's|^Exec=cosmic-sticky|Exec={{bin-exec}}|' resources/{{appid}}.desktop > {{desktop-dst}}
    sed 's|^Exec=.*|Exec={{bin-exec}}|' resources/autostart.desktop > {{autostart-dst}}

# Removes what install put in place; notes are kept.
uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{icon-dst}} {{autostart-dst}}
