# z-ro cowork — dev notes

## Running the app

```
npm run tauri dev
```

No special env setup needed. The macOS dyld cache provides
`libswift_Concurrency.dylib` for `screencapturekit`'s Swift FFI bridge.

## Running cargo tests

The lib test binary runs outside Tauri's bundling, so dyld doesn't find the
cached Swift runtime the same way the main binary does. You'll see:

```
dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
```

Set `DYLD_FALLBACK_LIBRARY_PATH` to the Command Line Tools Swift backport:

```
DYLD_FALLBACK_LIBRARY_PATH=/Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx \
  cargo test --lib
```

Or export it in your shell's profile so the full 170-test suite runs with a
plain `cargo test --lib`.

**Do not** bake this path into an rpath on the main binary — it's also mapped
into the dyld cache, so both copies load and produce
`Class _TtCs... is implemented in both … This may cause mysterious crashes.`
which crashed the app in an earlier iteration.
