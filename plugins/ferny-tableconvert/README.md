# ferny-tableconvert

Plugin para Ferny que convierte listas (texto delimitado) en tablas formateadas para múltiples formatos.

## Origen

Basado en el plugin **tableconvert** de [Geany-Plugins](https://plugins.geany.org/tableconvert.html) (versión 2.1), escrito en C por Frank Lanitz.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK
- **API**: Usa `ferny.editor.selection()` y `ferny.editor.replace_selection()` para operar sobre la selección
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **UI**: Sin diálogo gráfico; los formatos se exponen como comandos individuales
- **Delimitador**: Detección automática (tab, coma, punto y coma, pipe, espacio) + comando para configurar manualmente
- **Formatos soportados**: LaTeX, HTML, SQL, Markdown, CSV, DokuWiki, MediaWiki, reStructuredText

## Funcionalidad implementada

- Parseo de texto delimitado en filas/columnas
- Conversión a 8 formatos de tabla
- Detección automática de delimitador
- Comando "Auto-detect and Convert to Markdown"
- Soporte opcional de cabecera (primera fila)

## Formatos soportados

| Formato | Comando | Cabecera |
|---------|---------|----------|
| LaTeX | `tableconvert-latex` | Sí |
| HTML | `tableconvert-html` | Sí |
| SQL INSERT | `tableconvert-sql` | No |
| Markdown | `tableconvert-markdown` | Sí |
| CSV | `tableconvert-csv` | Sí |
| DokuWiki | `tableconvert-dokuwiki` | Sí |
| MediaWiki | `tableconvert-mediawiki` | Sí |
| reStructuredText | `tableconvert-rst` | Sí |

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-tableconvert`
3. Habilitar y conceder permisos si se solicitan

## Uso

1. Seleccionar texto delimitado (p.ej., columnas separadas por tabs, comas, etc.)
2. Tools → Table Convert → elegir formato
3. El texto seleccionado se reemplaza por la tabla formateada

Ejemplo entrada (tab-delimitado):
```
Nombre    Edad    Ciudad
Ana       28      Madrid
Juan      35      Barcelona
```

Salida Markdown:
```
| Nombre | Edad | Ciudad |
|---|---|---|
| Ana | 28 | Madrid |
| Juan | 35 | Barcelona |
```

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)