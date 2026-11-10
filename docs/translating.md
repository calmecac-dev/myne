# Translating

Myne uses gettext with one domain, `myne`. The strings in the code and in the
Blueprint files are written in English; translations live in `po/`, one `.po`
file per language, listed in `po/LINGUAS`. Spanish is the first one.

## Adding or updating a translation

```sh
po/i18n.sh update                              # refresh po/myne.pot and every .po
msginit -l fr -i po/myne.pot -o po/fr.po       # start a new language
echo fr >> po/LINGUAS
po/i18n.sh install                             # compile into ~/.local/share/locale
LANGUAGE=fr cargo run --release                # try it
```

Myne takes the translations from the first XDG data directory that has them,
so `~/.local/share/locale` wins over the system one during development.

Messages with `{name}` get a value at runtime; keep the placeholder as it is in
the translation. Plurals use `msgid_plural`. `msgfmt --check`, run by
`po/i18n.sh install`, rejects a plural form that is missing.

A new language also goes in `LANGUAGES` in `src/preferences.rs` and in the
`StringList` of the language row in `data/ui/preferences-dialog.blp`, in the
same order and named in itself.

## Marking strings

- **Rust:** wrap the text in `gettext("...")`, or `ngettext` for plurals. Put
  values in with `.replace("{name}", value)`. Explain a placeholder in a
  `// TRANSLATORS:` comment on the line before. xgettext does not look inside
  `glib::clone!`, so translatable text must sit outside that macro.
- **Blueprint:** `_("...")`, as usual.
- **New file with strings:** add it to `po/POTFILES`.
- Messages that groff itself writes are not translated.

## Desktop file

`data/dev.calmecac.Myne.desktop` carries its own `[es]` entries; it is not
extracted into the template.
