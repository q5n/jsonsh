#!/bin/bash

cd $(dirname "${BASH_SOURCE[0]}")

cargo clean
cargo build --release

sleep 3
