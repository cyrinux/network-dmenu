default: build

build:
    cargo build

test:
    cargo test
    cargo test --no-default-features

lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings

check: lint test

deny:
    cargo deny check

# Copy all sources to the clipboard (handy for sharing context)
list-files:
    @for file in src/*; do \
        echo "$(basename $file):"; \
        echo; \
        cat $file; \
        echo; \
    done | wl-copy
