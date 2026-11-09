# Pull request screenshots

Before-and-after screenshots that pull requests link to, kept on `main` so the
links keep working. Each folder belongs to one pull request:

| Folder | Pull request |
|---|---|
| `ui-header/` | #136, flat header bar in a proportional UI font |
| `antialiased-shapes/` | #137, antialiased rounded corners |
| `sidebar/` | #138, tab sidebar drawn like the header bar |
| `status/` | #139, status bar in the header's colors |
| `overlay-screenshots/` | #140, search box and command palette as popovers |
| `rename-screenshots/` | #141, palette kept open to rename a tab |
| `gpu-images/` | images drawn as GPU textures, beside software |
| `gpu-settings/` | the Settings window drawn on the GPU, beside software |
| `light-brights/` | bright colors (bold prompts) readable on light themes |
| `window-buttons/` | Linux window buttons in the header: light and dark, hovered and pressed; `auto` on sway (none) and `client` with the portal's `appmenu:close` |
| `chrome-colors/` | chrome colors derived from the theme, muted inactive tabs and 8 px padding, before and after in poop, darkmode and light-mode |
| `tab-bar/` | the header's new shape: 40 px high, the active tab joined to the terminal with rounded top corners, before and after in poop and light-mode |
| `palette-hints/` | each palette command's shortcut shown at its row's right end; the palette drawn from its model, since keystrokes cannot reach the window in the capture setup |
| `settings-rows/` | the Settings main page with its rows laid out from one table of gaps, before and after in poop and light-mode; drawn from the page's model, since keystrokes cannot open the window in the capture setup |
| `about-page/` | the About page centered with the name, release and tagline above quiet notices, before and after in poop and light-mode; drawn from the page's model, since keystrokes cannot open the window in the capture setup |

New screenshots go into a new folder here, in the same pull request they
illustrate, never on a separate branch; see
[visual checks](../testing.md#visual-checks).
