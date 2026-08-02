# Git Quoted Octal Range Design

## Problem

The quoted Git path decoder accepts any three octal digits but folds them directly into `u8`. Values from `\\400` through `\\777` therefore panic in debug builds and wrap to an incorrect byte in release builds.

## Decision

Parse a validated three-digit escape into `u16`, then convert it to `u8` with a checked conversion. Values above `0o377` return the existing `invalid octal Git path escape` diagnostic. Valid escape behavior and UTF-8 validation remain unchanged.

Add focused boundary examples for out-of-range escapes and a property test asserting that decoding any quoted Rust string is total and never panics.

## Compatibility

Wrapped out-of-range bytes are malformed input, not supported behavior. They are rejected consistently in debug and release builds.
