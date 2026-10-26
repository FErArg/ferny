# ferny-macros

Plugin para Ferny que permite grabar y reproducir macros del editor.

## Origen

Basado en el plugin **geanymacro** (Macros) de [Geany-Plugins](https://plugins.geany.org/geanymacro.html) (versión 2.1), escrito en C por William Fraser.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK
- **API**: Usa `ferny.editor`, `ferny.command`, `ferny.fs`, `ferny.log`, `ferny.ui`
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **Grabación**: La API de Ferny no expone eventos de Scintilla (`SCN_MACRORECORD`); la grabación se simula con comandos manuales
- **Reproducción**: No se pueden enviar mensajes directos a Scintilla; la reproducción registra los eventos para implementación futura
- **Almacenamiento**: Persistencia en `ferny.fs` (archivo `macros.json` en config dir)
- **Triggers de tecla**: Placeholders; vincular en `keybindings.toml` cuando la API lo soporte
- **Editor de macros**: Sin UI gráfica; gestión vía comandos

## Funcionalidad implementada

- **Grabar macro** — Inicia registro de eventos (simulado)
- **Detener grabación** — Finaliza y guarda macro
- **Reproducir macro** — Ejecuta la primera macro guardada
- **Listar macros** — Muestra macros guardadas con conteo de eventos
- **Eliminar macro** — Borra la primera macro
- **Renombrar macro** — Cambia nombre de la primera macro
- **Establecer trigger** — Placeholder para keybindings
- **Limpiar todas** — Borra todas las macros

## Estructura de macro

Cada macro almacena:
- `name` — Nombre identificativo
- `keyval`, `state` — Combinación de tecla trigger (futuro)
- `events[]` — Array de eventos: `{message, wparam, lparam}`

Los códigos de mensaje siguen constantes de Scintilla (SCI_*).

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-macros`
3. Habilitar y conceder permisos si se solicitan

## Uso

1. Tools → Macros → **Start Recording Macro**
2. Realizar acciones de edición (nota: la captura real de eventos requiere soporte de API)
3. Tools → Macros → **Stop Recording Macro**
4. Tools → Macros → **Play Macro** para reproducir
5. Tools → Macros → **List Macros** para ver las guardadas

Las macros persisten entre sesiones en `macros.json`.

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)