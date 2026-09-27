# Worth knowing

> What stood out while reading koi?

This guide explains; it doesn't review. Three things came up while writing it that are worth knowing before you change koi.

The web page repeats some of the terminal's defaults as its own constants. The performance numbers in the docs were measured on prototypes, not on the finished game. And the Windows build is compiled and tested but has never been watched running.

::: medium
### The web page keeps its own copy of some defaults

The number of koi, the water's grid and its frame rate are constants in `koi-web` (`crates/koi-web/src/lib.rs:19-22`), and the default volume is a number in `site/main.js` (`site/main.js:336`). They match the terminal's defaults today. The terminal's defaults live in the binary, where the web build can't read them, and no test compares the two. A change to one side needs the same change on the other.

### The performance numbers describe prototypes

`docs/PERFORMANCE.md` measured the designs koi was built from and says so (`docs/PERFORMANCE.md:92`). Nothing measures the finished game, and no CI job fails when it gets slower. The `d` stats line is the tool for looking at it (see Performance).

### Windows has never been watched

CI builds koi on Windows and runs its tests there (`.github/workflows/ci.yml:38-39`), and the architecture doc marks the port as experimental (`docs/ARCHITECTURE.md:112`). Nobody has run it in a Windows terminal and looked at the pond.
:::
