# Ferny Plugins

Directorio que contiene los plugins de Ferny adaptados desde Geany-Plugins.

## Plugins incluidos

| Plugin | Origen Geany | Funcionalidad |
|--------|-------------|---------------|
| `ferny-autoclose` | autoclose | Auto-cierra corchetes, comillas y etiquetas |
| `ferny-extraselection` | extrasel | Selección columnar, seleccionar hasta línea/llave/ancla |
| `ferny-tableconvert` | tableconvert | Convierte listas a tablas (LaTeX, HTML, SQL, Markdown, etc.) |
| `ferny-macros` | geanymacro | Grabar y reproducir macros del editor |
| `ferny-addons` | addons | Utilidades varias (strip blank lines, copy path, tasks, XML tagging) |
| `ferny-lipsum` | lipsum | Generador de texto Lorem Ipsum |
| `ferny-automark` | automark | Resaltar palabra bajo cursor |
| `ferny-lineoperations` | lineoperations | Operaciones de línea (duplicados, vacías, ordenar) |
| `ferny-shiftcolumn` | shiftcolumn | Desplazar caracteres entre columnas |
| `ferny-overview` | overview | Navegación y vista general |
| `ferny-defineformat` | defineformat | Formatear definiciones, alinear asignaciones |
| `ferny-pairtaghighlighter` | pairtaghighlighter | Resaltar etiquetas HTML/XML coincidentes |
| `ferny-geanyinsertnum` | geanyinsertnum | Insertar números secuenciales |
| `ferny-xmlsnippets` | xmlsnippets | Snippets XML/HTML |
| `ferny-markdown` | markdown | Convertir MD/HTML, TOC, vista previa |
| `ferny-spellcheck` | spellcheck | Corrección ortográfica básica |
| `ferny-pretty-printer` | pretty-printer | Formatear código (indentación) |
| `ferny-scope` | scope | Navegación por ámbitos/bloques |
| `ferny-geniuspaste` | geniuspaste | Pegar en pastebin/Gist |
| `ferny-keyrecord` | keyrecord | Grabar/reproducir pulsaciones de teclas |
| `ferny-updatechecker` | updatechecker | Comprobar actualizaciones |
| `ferny-utils` | utils | Utilidades varias (contar, mayúsculas, invertir, etc.) |

## Instalación

### Método 1: Script de instalación (recomendado)

```bash
# Desde el repositorio ferny-plugins
./scripts/install-ferny-plugins.sh

# O especificando directorios
./scripts/install-ferny-plugins.sh /ruta/a/ferny-plugins ~/.config/ferny/plugins
```

### Método 2: Manual

```bash
# Copiar todos los plugins
cp -r ferny-*/ ~/.config/ferny/plugins/

# O plugin por plugin
mkdir -p ~/.config/ferny/plugins
cp -r ferny-autoclose ~/.config/ferny/plugins/
cp -r ferny-extraselection ~/.config/ferny/plugins/
# ... etc
```

### Método 3: Empaquetado en Ferny (bundled)

Para compilar los plugins dentro del binario de Ferny, añadirlos al array `BUNDLED` en:
`/tmp/ferny/crates/ferny-plugin/src/lib.rs`

## Uso

1. Iniciar Ferny
2. Tools → Plugins…
3. Buscar los plugins instalados
4. Habilitar y conceder permisos
5. Reiniciar Ferny

## Estructura de cada plugin

```
ferny-<nombre>/
├── plugin.toml      # Metadatos (id, name, version, permissions)
├── main.lua         # Lógica principal
├── locale/
│   └── es.toml      # Traducciones al español
└── README.md        # Documentación del plugin
```

## Permisos requeridos

| Plugin | Permisos |
|--------|----------|
| `ferny-autoclose` | ninguno |
| `ferny-extraselection` | ninguno |
| `ferny-tableconvert` | ninguno |
| `ferny-macros` | ninguno |
| `ferny-addons` | clipboard |
| `ferny-lipsum` | ninguno |
| `ferny-automark` | ninguno |
| `ferny-lineoperations` | ninguno |
| `ferny-shiftcolumn` | ninguno |
| `ferny-overview` | ninguno |
| `ferny-defineformat` | ninguno |
| `ferny-pairtaghighlighter` | ninguno |
| `ferny-geanyinsertnum` | ninguno |
| `ferny-xmlsnippets` | ninguno |
| `ferny-markdown` | ninguno |
| `ferny-spellcheck` | ninguno |
| `ferny-pretty-printer` | ninguno |
| `ferny-scope` | ninguno |
| `ferny-geniuspaste` | clipboard, process.spawn |
| `ferny-keyrecord` | ninguno |
| `ferny-updatechecker` | ninguno |
| `ferny-utils` | ninguno |

## Configuración persistente

Cada plugin puede guardar configuración en archivos JSON dentro del directorio del plugin:
- `ferny-addons`: `addons_config.json`
- `ferny-lipsum`: `lipsum_config.json`

Estos archivos se crean automáticamente en `~/.config/ferny/plugins/<plugin-id>/`.

## Desinstalación

```bash
# Eliminar todos los plugins
rm -rf ~/.config/ferny/plugins/ferny-*

# O plugin por plugin
rm -rf ~/.config/ferny/plugins/ferny-autoclose
# ... etc
```

## Migración desde Geany

Los plugins están adaptados desde Geany-Plugins 2.1. Algunas funcionalidades pueden ser placeholders debido a limitaciones de la API de Ferny:

- `ferny-autoclose`: el auto-cierre al escribir requiere soporte de keypress events
- `ferny-extraselection`: selección rectangular y anclas son placeholders
- `ferny-macros`: grabación real de eventos requiere API de Scintilla
- `ferny-addons`: mark word y open URI son placeholders
- `ferny-automark`: resaltado requiere API de indicadores
- `ferny-pairtaghighlighter`: resaltado requiere API de indicadores
- `ferny-scope`: navegación por ámbitos requiere API de editor
- `ferny-geniuspaste`: requiere permiso process.spawn

Ver `README.md` en cada plugin para más detalles.