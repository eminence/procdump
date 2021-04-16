procdump
========

A linux command-line tool to display information about a running process

**Note** Under development, but please try it out and provide feedback

# Install

Install the latest published version:

> cargo install procdump

Or clone and build from source.  Install [rust](https://rustup.rs/), download the source, and then run:

> cargo build

# Usage

```
procdump [PID]
```

If the `PID` argument is missing, procdump will show information
about its own running process.
