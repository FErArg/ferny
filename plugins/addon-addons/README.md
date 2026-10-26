# ferny-addons

Plugin para Ferny que proporciona una colección de pequeñas utilidades (addons).

## Origen

Basado en el plugin **Addons** de [Geany-Plugins](https://plugins.geany.org/addons.html) (versión 2.1), escrito en C por:
- Enrico Tröger
- Bert Vermeulen
- Eugene Arshinov
- Frank Lanitz

El plugin original de Geany es una colección de 9 sub-plugins que se pueden habilitar/deshabilitar individualmente desde las preferencias.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK/GObject
- **API**: Usa la API de plugins de Ferny (`ferny.editor`, `ferny.command`, `ferny.documents`, `ferny.on`, `ferny.fs`, `ferny.clipboard`, `ferny.ui`, `ferny.log`, `ferny.tr`)
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **Arquitectura**: Simplificada; en lugar de objetos GObject con propiedades, usa una tabla de configuración simple en Lua
- **Persistencia**: Configuración guardada en JSON via `ferny.fs` en lugar de GKeyFile
- **Eventos**: Usa `ferny.on()` para eventos de documento en lugar de callbacks de Geany
- **UI**: Sin diálogo de preferencias gráfico; la configuración se alterna vía comandos

## Sub-plugins implementados

| Funcionalidad | Geany Addons | Ferny Addons | Estado |
|---------------|-------------|--------------|--------|
| Strip trailing blank lines | ✅ | ✅ | Implementado |
| Mark word on double-click | ✅ | ⚠️ | Placeholder (API pendiente) |
| Copy file path | ✅ | ✅ | Implementado |
| Tasks (TODO/FIXME) | ✅ | ✅ | Implementado |
| Open URI | ✅ | ⚠️ | Placeholder (API pendiente) |
| Bookmark list | ✅ | ❌ | No aplicable (sidebar) |
| XML tagging | ✅ | ✅ | Básico implementado |
| Enclose words | ✅ | ✅ | Implementado (ver también ferny-autoclose) |
| Color tip | ✅ | ❌ | No aplicable |
| Systray | ✅ | ❌ | No aplicable |

## Comandos disponibles

### Archivo
- **Copy File Path** — Copia la ruta del archivo actual al portapapeles

### Edición
- **Show Tasks (TODO/FIXME)** — Escanea el documento actual y muestra líneas con TODO/FIXME
- **Open URI Under Cursor** — Placeholder para abrir URI bajo el cursor
- **XML Tagging for Selection** — Envuelve la selección en `<tag>...</tag>`
- **Enclose Selection with Brackets/Square/Curly/Quotes** — Encierra selección (ver también ferny-autoclose)

### Configuración (toggle)
- **Toggle: Strip Trailing Blank Lines on Save** — Activa/desactiva eliminación de líneas en blanco finales al guardar
- **Toggle: Mark Word on Double-Click** — Activa/desactiva marcado de palabra (placeholder)
- **Toggle: Single-Click Deselect Marked Word** — Activa/desactiva des-selección con clic simple

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-addons`
3. Habilitar y conceder permisos (clipboard)

## Uso

Una vez activado, los comandos están disponibles en Tools → Addons:

- **Copy File Path** — Atajo útil: vincular en `keybindings.toml`
- **Show Tasks** — Escanea TODO/FIXME en el documento actual
- **XML Tagging** — Seleccionar texto y ejecutar para envolver en `<tag>`
- **Enclose Selection** — Varias opciones de encerrado

La configuración persiste entre sesiones en `addons_config.json`.

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)