# imagenes

Images and logos. Never embed secrets or personal data.

| File | Use |
|---|---|
| `ferny-icon-app.png` (257×256) | Application icon and logo: window, taskbar and Dock icon (Qt resource `:/ferny-icon-app.png`); source of `packaging/windows/ferny.ico` |
| `ferny-logo-removebg-preview.png` (248×227) | Large logo with the name, shown in Help → About Ferny (Qt resource `:/ferny-logo-removebg-preview.png`) |

Both files are stored without metadata: the originals carried an EXIF
profile (embedded thumbnail) and timestamps in PNG text chunks, removed
before committing; the pixels are unchanged.

`packaging/windows/ferny.ico` is derived from `ferny-icon-app.png`
(centre crop to 256×256 with `sips -c 256 256`, then 16, 24, 32, 48, 64,
128 and 256 px PNG entries in one ICO file). Regenerate it whenever the
icon changes.

Pending for packaging: a 1024×1024 version of the icon for the macOS
`.icns` (Retina sizes) and Linux high-DPI icons; scaling the 256 px icon
up would look blurry.

## Lucide icons

`lucide/` contains the official Lucide SVG sources used by Ferny's toolbar and
About dialog. They are sourced from `https://github.com/lucide-icons/lucide`
and distributed under the ISC License; see the repository-root `NOTICE`.
Ferny vendors only the icons it currently uses. The complete icon catalog is
available from the upstream Lucide project.
