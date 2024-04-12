# Incrementor

A simple version bumping tool designed to efficiently handle multiple version numbers across various packages,
libraries, targets in monorepo setup. The focus lies in the flexibility to define all the necessary files in a
configuration file and update them with a user-friendly CLI.

For example:
```toml
#incrementor.toml
current_version = "0.1.0"

[files.VERSION]
search = "{current_version}"
replace = "{new_version}"

[files.'package.json']
search = '"version": "{current_version}"'
replace = '"version": "{new_version}"'

[files.'Cargo.toml']
search = 'version = "{current_version}"$'
replace = 'version = "{new_version}"'
```

Run incrementor to increment the version from `0.1.0` to `0.2.0`
```shell
incrementor --minor
```

## Git Commit & Tag
Incrementor isn't just a version incrementor. It can also automatically generate a commit and tag in Git based on the
command-line command and configurations.

# Installation

```shell
cargo install --locked incrementor
```

# Usage
```shell
incrementor --minor 
```