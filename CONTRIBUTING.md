# Contributing to Lexpeek

## Development

Build and install a development version with `./install.sh` as described in [Building from source](README.md#building-from-source), and run the tests with `cargo test`.

The domain terms used in the code are defined in [GLOSSARY.md](GLOSSARY.md).

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `feat: ...`, `fix: ...`, `docs: ...`.

## Project structure

```
src/
  domain/          domain model: languages, text, dictionary article
  application/     use cases and the ports they declare
  infrastructure/  port implementations: translators, recognition, speech, GNOME portals, settings
  ui/              GTK interface
  main.rs          dependency wiring
locales/           interface translations
data/              desktop entry and icon
```

## Adding a translator

Implement the `Translator` trait in `src/infrastructure/`, add it to the `available` list in `src/main.rs` and its name to `locales/*.yml` under the `translators.<id>` key. The choice in the preferences, language detection and the dictionary article are picked up automatically.

## Adding an interface language

Copy `locales/en.yml` to `locales/<code>.yml` and translate the strings; the language appears in the preferences automatically. `cargo test` checks that all translations have the same set of strings.

## Adding an action

Add a variant to `Action` in `src/ui/action.rs` and handle it in `App::perform`. The shortcut and the menu item come from the list of actions.

## Packaging and releases

The `.deb` package is built with [cargo-deb](https://github.com/kornelski/cargo-deb):

```sh
cargo install cargo-deb
cargo deb
```

The package appears in `target/debian/`. Pushing a `v*` tag makes GitHub Actions run the tests, build the package and attach it to a new release.
