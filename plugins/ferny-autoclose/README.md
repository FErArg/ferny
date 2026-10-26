# ferny-autoclose

Plugin para Ferny que auto-cierra corchetes, comillas y etiquetas, y permite encerrar selecciones.

## Origen

Basado en el plugin **autoclose** de [Geany-Plugins](https://plugins.geany.org/autoclose.html) (versión 2.1), escrito en C por Pavel Roschin y otros contribuyentes.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK
- **API**: Usa la API de plugins de Ferny (`ferny.editor`, `ferny.command`, `ferny.on`, etc.)
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **Eventos**: No hay acceso directo a eventos de teclado; se proporcionan comandos que el usuario puede vincular en `keybindings.toml`
- **Configuración**: Simplificada; las opciones complejas del original (estilos de indentación, comportamiento en comentarios, etc.) no están implementadas en esta versión inicial

## Funcionalidad implementada

- Auto-cierre de pares: `()`, `[]`, `{}`, `""`, `''`, ` `` `, `<>`
- Encerrar selección con paréntesis, corchetes, llaves, comillas dobles/simples
- Comandos disponibles para vincular en `keybindings.toml`

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-autoclose`
3. Habilitar y conceder permisos si se solicitan

## Uso

Una vez activado, los comandos están disponibles en Tools → Auto Close:

- **Enclose Selection with Brackets** — Encierra selección con `()`
- **Enclose Selection with Square Brackets** — Encierra con `[]`
- **Enclose Selection with Curly Braces** — Encierra con `{}`
- **Enclose Selection with Double Quotes** — Encierra con `""`
- **Enclose Selection with Single Quotes** — Encierra con `''`

Para auto-cierre al escribir, vincular comandos en `keybindings.toml` (requiere soporte de la API para keypress events).

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)