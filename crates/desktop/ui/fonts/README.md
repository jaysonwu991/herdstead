# Bundled terminal font

`JetBrainsMonoNLNerdFontMono-Regular.ttf` is JetBrains Mono NL Nerd Font Mono
from [Nerd Fonts v3.5.1](https://github.com/ryanoasis/nerd-fonts/tree/v3.5.1/patched-fonts/JetBrainsMono/NoLigatures).
It includes text, Powerline separators and common Nerd Font prompt glyphs. The
monospaced, non-ligature variant preserves the terminal grid. The browser
synthesises bold and italic styles.

The font is served from the app's local assets and requires no network or
system font installation. Settings can select a different installed font;
the bundled font remains the fallback for missing symbols. The client waits
for it to load before measuring cells and attaching to the PTY.

See [OFL.txt](OFL.txt) for the upstream SIL Open Font License and attribution.
