# z-ro cowork — dev notes

## Running the app

```
npm run tauri dev
```

No special env setup needed. [src-tauri/.cargo/config.toml](.cargo/config.toml)
adds `/usr/lib/swift` as an rpath so `screencapturekit`'s Swift FFI bridge
resolves `@rpath/libswift_Concurrency.dylib` via the macOS dyld shared cache.

## Running cargo tests

Plain `cargo test --lib` — same rpath applies, tests find Swift runtime.
(170 tests at time of writing.)

## Why the rpath is what it is

`screencapturekit`'s Swift FFI links our main binary against
`@rpath/libswift_Concurrency.dylib`. Dyld's default search path doesn't
include `/usr/lib/swift/`, so without an rpath the main binary fails to
launch:

```
dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
tried: '/usr/lib/libswift_Concurrency.dylib' (not in dyld cache)
```

An earlier iteration used `/Library/Developer/CommandLineTools/usr/lib/
swift-5.5/macosx` as the rpath. It fixed the load, but some other dep in
the tree loads `/usr/lib/swift/libswift_Concurrency.dylib` through a
hardcoded `LC_LOAD_DYLIB`. Those are two different on-disk paths → two
dylib mappings → two copies of the same Obj-C classes → mysterious
crashes mid-session:

```
objc[…]: Class _TtCs25CheckedContinuationCanary is implemented in both
  /usr/lib/swift/… and /Library/Developer/CommandLineTools/…
  This may cause spurious casting failures and mysterious crashes.
```

Setting rpath to `/usr/lib/swift` means both our resolution and the
hardcoded path resolve to the **same** file. Dyld dedupes on full path,
so there's one mapping, one class registration, no crash.
