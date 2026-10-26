# ferny-extraselection

Plugin para Ferny que añade selección columnar (rectangular), selección hasta línea/llave/ancla.

## Origen

Basado en el plugin **extrasel** (Extra Selection) de [Geany-Plugins](https://plugins.geany.org/extrasel.html) (versión 2.1), escrito en C por Dimitar Toshkov Zhekov.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK
- **API**: Usa la API de plugins de Ferny (`ferny.editor`, `ferny.command`, `ferny.documents`, etc.)
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **Eventos**: No hay acceso directo a eventos de teclado ni a Scintilla; se proporcionan comandos que el usuario puede vincular en `keybindings.toml`
- **Modo rectangular**: La API de Ferny no expone directamente el modo de selección rectangular; los comandos son placeholders para cuando la API lo soporte
- **Ancla de selección**: Implementada con variable local en Lua (persiste solo durante la sesión)

## Funcionalidad implementada

- **Selección hasta inicio/final de línea** — Placeholder para keybindings
- **Selección hasta llave correspondiente** — Busca la llave `()`, `[]`, `{}` correspondiente
- **Ancla de selección** — Set/clear/select to anchor (variable de sesión)
- **Selección rectangular** — Placeholder (toggle)
- **Selección hasta inicio/final de documento** — Placeholders
- **Selección párrafo arriba/abajo** — Placeholders

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-extraselection`
3. Habilitar y conceder permisos si se solicitan

## Uso

Una vez activado, los comandos están disponibles en Tools → Extra Selection:

- **Select to Line Start** — Seleccionar hasta inicio de línea
- **Select to Line End** — Seleccionar hasta fin de línea
- **Select to Matching Brace** — Seleccionar hasta llave correspondiente
- **Set Selection Anchor** — Establecer ancla en posición actual
- **Select to Anchor** — Seleccionar desde ancla hasta posición actual
- **Clear Selection Anchor** — Limpiar ancla
- **Toggle Rectangular Selection** — Alternar modo rectangular
- **Select to Document Start/End** — Seleccionar hasta inicio/fin documento
- **Select Paragraph Up/Down** — Seleccionar párrafo anterior/siguiente

Vincular en `keybindings.toml` para uso eficiente.

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)