# Messages for angzarr-fmt's diagnostics. Each id is a diagnostic's code; the
# variables are its args.

angzarr-fmt-unknown-format = unknown format { $name }: use json, yaml, toml, text or markdown
# $detail is the serializer's own English text: data to show as read, not to
# translate.
angzarr-fmt-serialize = the result could not be serialized: { $detail }
# $found is a token: list, string, number, boolean or null.
angzarr-fmt-toml-needs-table = TOML output needs a table at the top level, not { $found ->
        [list] a list
        [string] a string
        [number] a number
        [boolean] a boolean
       *[null] null
    }
angzarr-fmt-toml-null = TOML has no null: the value at { $path } is empty
angzarr-fmt-toml-int-range = TOML integers are 64-bit signed: the value at { $path } is too large
# $detail is the TOML encoder's own English text: data to show as read, not to
# translate.
angzarr-fmt-toml-encode = the result could not be written as TOML: { $detail }
# $kind is an I/O error kind as a kebab-case token (broken-pipe, storage-full,
# other, ...).
angzarr-fmt-write = the output could not be written: { $kind }
# $kind is the Fluent parser's error kind as a kebab-case token.
angzarr-fmt-catalog-parse = a message catalog has a syntax error at line { $line }, column { $column }: { $kind }
angzarr-fmt-catalog-duplicate = two message catalogs define { $id }

## Remedies.

angzarr-fmt-use-json-or-yaml = write this output as json or yaml instead
