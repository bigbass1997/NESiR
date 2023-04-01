### Description
`NESiR` (NES in Rust) is yet another NES emulator. Not intended for public use. Don't expect regular updates or help troubleshooting.

This project will fail if run through `miri` as I used mutable aliasing (via UnsafeCell). This practice is very unsafe and should NOT be used as an example for your own projects!

The CPU currently passes nestest's automated test. Haven't tried [Tom Harte's JSON test](https://github.com/TomHarte/ProcessorTests) yet, but I'd suspect I'd fail it.
