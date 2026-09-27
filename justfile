# Common tasks. `just` lists them.

# Tests compare the GPU and CPU renderers on lavapipe, a Vulkan driver on the CPU, when it is
# installed, so they never need a real GPU.
lavapipe := if path_exists("/usr/share/vulkan/icd.d/lvp_icd.json") == "true" { "VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json" } else { "" }

# List the recipes
default:
    @just --list

# Run koi from source, with any arguments, such as `just run --theme moonlit-pond`
run *args:
    cargo run --release -- {{args}}

# Run the tests
test:
    {{lavapipe}} cargo test --workspace

# Check formatting and lints
lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings

# Format the code
fmt:
    cargo fmt

# What CI checks on Linux: formatting, lints, tests and docs (CI also builds on the oldest supported Rust)
check: lint test
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Install the web page's build tools: wasm-bindgen-cli at the version in Cargo.lock, and the wasm target
web-tools:
    rustup target add wasm32-unknown-unknown
    cargo install --locked wasm-bindgen-cli --version $(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
    @echo "wasm-opt is optional: install binaryen for a smaller, faster wasm."

# Build the web page into target/site
site:
    scripts/build-site.sh

# Build the web page and serve it at http://127.0.0.1:PORT
serve port="8123": site
    @echo "http://127.0.0.1:{{port}}/"
    python3 -m http.server {{port}} --bind 127.0.0.1 -d target/site

# Check the built web page in headless Chrome, saving screenshots when given a folder
smoke shots="":
    node scripts/site-smoke.mjs {{ if shots == "" { "" } else { "--shots " + shots } }}

# Screenshot koi in Ghostty on a private virtual display, such as `just shot /tmp/shot 8 --theme pixel`
shot out seconds *args:
    cargo build --release
    scripts/vshot.sh {{out}} {{seconds}} {{justfile_directory()}}/target/release/koi {{args}}

# Download the full music set into ~/.local/share/koi-pond/music
music:
    scripts/fetch-music.sh

# Build the release archives for this machine into dist/
release *args:
    scripts/release.sh {{args}}
