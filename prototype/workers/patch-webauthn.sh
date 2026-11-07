#!/bin/sh
# webauthn-rs-core 0.6.2 calls std::time::SystemTime::now(), which panics on
# wasm32-unknown-unknown. Fetch the published crate, check it, and apply a
# small patch that reads the clock from JavaScript instead. Cargo.toml
# points [patch.crates-io] at the result.
set -eu
cd "$(dirname "$0")"
dest=vendor/webauthn-rs-core
[ -f "$dest/.patched" ] && exit 0
sum=6d13e555c106095051abd70415b88546a0d3757a7990e05b4659b4fba19e46ac
rm -rf vendor && mkdir -p vendor
curl -sSfL https://static.crates.io/crates/webauthn-rs-core/webauthn-rs-core-0.6.2.crate -o vendor/crate.tgz
echo "$sum  vendor/crate.tgz" | sha256sum -c - >/dev/null
tar -xzf vendor/crate.tgz -C vendor
mv vendor/webauthn-rs-core-0.6.2 "$dest"
rm vendor/crate.tgz
patch -s -p1 -d "$dest" < patches/webauthn-rs-core-wasm-clock.diff
touch "$dest/.patched"
