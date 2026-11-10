# Myne

Editor de escritorio para documentos **groff con macros mom**: editor a la
izquierda, vista previa del PDF a la derecha y registro de compilación abajo.
Al guardar, el documento se compila con `pdfmom` y la vista previa se
actualiza sin perder la página ni el zoom.

Solo Linux (Wayland y X11). Licencia GPL-3.0-or-later.

## Dependencias

Debian/Ubuntu:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libpoppler-glib-dev blueprint-compiler groff
```

Versiones mínimas: GTK 4.14, libadwaita 1.5, GtkSourceView 5.12, Poppler 0.82,
Rust 1.92. `groff` se usa del sistema y no se incluye con Myne; sin él, la
aplicación arranca igualmente y explica qué falta.

## Compilar y ejecutar

```bash
cargo run
```

```bash
cargo run -- documento.mom
```

Si `blueprint-compiler` del sistema no funciona (por ejemplo, porque `python3`
apunta a una versión sin PyGObject), se puede indicar otro ejecutable:

```bash
BLUEPRINT_COMPILER=/ruta/al/blueprint-compiler cargo build
```

## Cómo compila

- Comando: `pdfmom -k [-t] [-e] [-p] [-R] documento.mom`, con el directorio del
  documento como directorio de trabajo (funcionan `.so`, imágenes y rutas
  relativas). `-k` hace que groff lea el fuente como UTF-8.
- El PDF se escribe en un temporal oculto junto al fuente y, si todo va bien,
  se renombra a `documento.pdf`. Si la compilación falla, se conserva el PDF
  anterior.
- groff sale con estado 0 incluso cuando hay errores, así que cualquier
  mensaje de error en la salida cuenta como fallo.
- Solo hay una compilación a la vez: un guardado nuevo cancela la anterior
  (se mata todo su grupo de procesos).

## Preferencias

Se guardan en `~/.config/myne/config.toml`.

## Atajos

| Acción | Atajo |
|---|---|
| Abrir | Ctrl+O |
| Guardar (y compilar) | Ctrl+S |
| Guardar como | Ctrl+Mayús+S |
| Compilar sin guardar | F5 |
| Mostrar u ocultar el registro | Ctrl+Mayús+L |
| Acercar / alejar / ajustar al ancho | Ctrl++ / Ctrl+− / Ctrl+0 |
| Zoom con la rueda | Ctrl+rueda |
| Preferencias | Ctrl+, |
| Atajos de teclado | Ctrl+? |
| Salir | Ctrl+Q |

## Estructura

```
data/ui/*.blp        interfaz (Blueprint)
src/compiler.rs      ejecutar pdfmom y analizar sus mensajes (sin GTK, con pruebas)
src/viewer.rs        visor de PDF con Poppler y Cairo
src/window.rs        ventana principal
src/log_panel.rs     registro de compilación
src/preferences.rs   diálogo de preferencias
src/config.rs        preferencias en TOML
```
