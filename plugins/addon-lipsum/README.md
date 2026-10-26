# ferny-lipsum

Plugin para Ferny que genera texto de relleno (Lorem Ipsum / Lipsum).

## Origen

Basado en el plugin **Lipsum** (originalmente GeanyLipsum) de [Geany-Plugins](https://plugins.geany.org/lipsum.html) (versión 2.1), escrito en C por Frank Lanitz.

## Adaptación para Ferny

Este plugin ha sido **adaptado desde el código C original de Geany** para funcionar en Ferny usando la API de plugins Lua (API 0). Las principales diferencias:

- **Lenguaje**: Reescrito en Lua 5.4 en lugar de C/GTK
- **API**: Usa la API de plugins de Ferny (`ferny.editor`, `ferny.command`, `ferny.documents`, `ferny.fs`, `ferny.ui`, `ferny.log`, `ferny.tr`)
- **Sandbox**: Ejecuta en el sandbox de Ferny (sin `io`, `os`, `debug`, `coroutine`, `require`, `load`)
- **Configuración**: JSON via `ferny.fs` en lugar de GKeyFile
- **UI**: Sin diálogo numérico; la longitud se configura editando el archivo de configuración o usando el comando de longitud personalizada
- **Deshacer**: Ferny agrupa inserciones en una sola acción de deshacer automáticamente

## Funcionalidad

- Genera texto Lorem Ipsum de longitud configurable
- Texto Lipsum personalizable (editar archivo de configuración)
- Longitud por defecto: 1500 caracteres (igual que el original)
- Texto por defecto: Lorem Ipsum clásico
- Configuración persistente entre sesiones

## Comandos disponibles

| Comando | Descripción |
|---------|-------------|
| **Insert Lipsum Text** | Inserta 1500 caracteres de Lorem Ipsum en la posición del cursor |
| **Insert Lipsum Text (Custom Length)** | Inserta texto con longitud definida en config (`custom_length`) |
| **Set Custom Lipsum Text** | Placeholder: editar `lipsum_config.json` para cambiar el texto base |
| **Set Custom Lipsum Length** | Placeholder: editar `lipsum_config.json` para cambiar `custom_length` |
| **Reset Lipsum to Default** | Restaura texto y longitud por defecto |
| **Show Current Lipsum Text** | Muestra el texto Lipsum actual (preview) |

## Instalación

El plugin se incluye en la distribución de Ferny (compilado en el binario). Para activarlo:

1. Tools → Plugins…
2. Buscar `ferny-lipsum`
3. Habilitar

## Uso

1. Colocar el cursor donde se quiere insertar el texto
2. Tools → Lipsum → **Insert Lipsum Text**
3. Se insertan 1500 caracteres de Lorem Ipsum

Para longitud personalizada:
1. Editar `lipsum_config.json` (en el directorio de configuración de Ferny)
2. Añadir/editar: `"custom_length": 500`
3. Tools → Lipsum → **Insert Lipsum Text (Custom Length)**

Para texto personalizado:
1. Editar `lipsum_config.json`
2. Cambiar el valor de `"lipsum_text"`
3. Tools → Lipsum → **Insert Lipsum Text**

## Archivo de configuración (`lipsum_config.json`)

```json
{
  "lipsum_text": "Lorem ipsum dolor sit amet...",
  "custom_length": 1500
}
```

## Licencia

GPL-2.0-or-later (heredada del plugin original de Geany)