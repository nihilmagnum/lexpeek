# Lexpeek

Screen text translator for GNOME on Wayland. Select an area of the screen or text in any window, press a shortcut and get the translation with a dictionary article. It also works where text cannot be copied: in games, images and videos.

## Features

- **Screen area translation.** Text is recognized by Tesseract and translated.
- **Selected text translation.** A card with the translation, copying the translation to the clipboard, or replacing the selected text with its translation right in the input field.
- **Dictionary article** for words and short phrases: transcription, translation variants by part of speech, definitions, synonyms, examples.
- **Choice of translators:** Google Translate and Yandex Translate.
- **Speech** for the original and the translation with buttons on the card.
- **Language pair with auto-detection:** text in your native language is translated into the second one, text in any other language into your native one.
- **Recognition languages:** 13 built-in ones whose models are downloaded from the app. Any other language can be added from a `.traineddata` file.
- **Interface in English and Russian:** follows the system language by default and can be chosen in the preferences.
- Icon in the top bar, customizable shortcuts, start at login.

## Installation

Requires Ubuntu 26.04 or newer, GNOME on Wayland and the top bar indicator extension (enabled by default in Ubuntu).

Download `lexpeek_*.deb` from the [releases page](https://github.com/nihilmagnum/lexpeek/releases) and install it:

```sh
sudo apt install ./lexpeek_*.deb
```

Lexpeek appears in the application list. On first launch GNOME asks you to choose the shortcuts, and lexpeek downloads the recognition models for Russian and English, the default language pair.

## Usage

| Action | Default shortcut |
|---|---|
| Translate Screen Area | `Ctrl+Alt+R` |
| Translate Selected Text | `Ctrl+Alt+W` |
| Copy Translation of Selection | `Ctrl+Alt+C` |
| Replace Selection with Translation | `Ctrl+Alt+V` |

In screen area mode, select the part with text using the mouse; Esc closes the frame. The window with the translation of the selection closes on Esc or when you switch to another window. Copying and replacing work without windows, and a notification reports failures. For replacing, GNOME asks once for remote desktop permission.

Tray icon menu:
- **Preferences…:** interface language, translator, language pair, downloading and removing recognition models, adding a language from a file.
- **Keyboard Shortcuts…:** changing and clearing shortcuts.
- **Start at Login.**

## Data

| What | Where |
|---|---|
| Settings: interface language, language pair, custom languages, translator | `~/.config/lexpeek/settings.json` |
| Recognition models | `~/.local/share/lexpeek/tessdata` |
| Remote desktop permission | `~/.local/share/lexpeek/remote-desktop-token` |

Lexpeek grants itself the screenshot permission at startup: GNOME does not show the permission dialog to background applications.

Screenshots are not kept: the file the portal creates is deleted right after reading, and the screenshot itself stays in memory only while the frame is open. Only text is sent to the translator.

## Building from source

Requires GNOME 48 or newer, Rust 1.88 or newer and the build packages:

```sh
sudo apt install cargo libgtk-4-dev libtesseract-dev libleptonica-dev clang libclang-dev pkg-config \
    wl-clipboard gstreamer1.0-plugins-good
git clone https://github.com/nihilmagnum/lexpeek
cd lexpeek
./install.sh
```

The script builds lexpeek, installs it into `~/.cargo/bin` and adds the desktop entry and icon to `~/.local/share`.

The `.deb` package is built with `cargo deb` (requires `cargo install cargo-deb`), and the result appears in `target/debian/`. On a `v*` tag GitHub Actions builds the package and attaches it to the release.

## Architecture

```
src/
  domain/          languages, pair and catalog; text; dictionary article
  application/     use cases (reading and translation, languages, selected text, translator choice)
                   and the ports they declare, including desktop ports
  infrastructure/  Google, Yandex, Tesseract, settings, GNOME portals, wl-clipboard, autostart
  ui/              GTK: application, frame, card, windows, tray icon, interface language
  main.rs          dependency wiring
locales/           interface translations
```

To add a translator, implement the `Translator` trait in `src/infrastructure/`, add it to the `available` list in `src/main.rs` and its name to `locales/*.yml` under the `translators.<id>` key. The choice in the preferences, language detection and the dictionary article are picked up automatically.

To translate the interface into a new language, copy `locales/en.yml` to `locales/<code>.yml` and translate the strings; the language appears in the preferences automatically. `cargo test` checks that all translations have the same set of strings.

To add an action, add a variant to `Action` (`src/ui/action.rs`) and handle it in `App::perform`. The shortcut and the menu item come from the list of actions.

Tests: `cargo test`.

## Limitations

- **Translators.** Google and Yandex are used through unofficial free endpoints without a key. Their owners may restrict or close access.
- **Changing a shortcut.** It relies on an internal GNOME mechanism and may break after a GNOME update. The shortcuts themselves keep working.
- **Dictionary article.** Available only for words and short phrases. Yandex provides no definitions or examples.

## License

[MIT](LICENSE)
