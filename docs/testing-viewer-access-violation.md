# Windows viewer test access violation

Reproduced on Windows on 2026-10-15, before the Connect UI changes were compiled:

```text
cargo test -p effectcraft-ui-egui --test ui_viewer
process didn't exit successfully (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)
```

The process reported 56 successful tests before terminating; it did not report a
Rust assertion failure or identify the crashing test. The same symptom had been
observed during the preceding IK handle work. All 60 tests passed with
`-- --test-threads=1` during that work.

The root cause is not established. Parallelism is a reproduction condition, not
proof of a particular race or rendering backend defect. No unrelated test,
renderer, or concurrency changes have been made to hide the failure. Until a
native stack trace identifies its origin, retain both the parallel failure and
the serial test result when reporting verification.

After the Connect UI changes, the serial command again passed all 60 tests
(48.05 seconds). `cargo check -p effectcraft` also passed. This does not resolve
or explain the native parallel-process failure.
