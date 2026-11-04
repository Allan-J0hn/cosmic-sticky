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

install:
    install -Dm0755 {{ cargo-target-dir / 'release' / name }} {{bin-dst}}
    install -Dm0644 resources/{{appid}}.desktop {{desktop-dst}}
    install -Dm0644 resources/icons/{{appid}}.svg {{icon-dst}}

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{icon-dst}}
