# Messages for angzarr-fmt's diagnostics. Each id is a diagnostic's code; the
# variables are its args.

angzarr-fmt-unknown-format = unknown format { $name }: use json, yaml, toml, text or markdown
angzarr-fmt-serialize = the result could not be serialized: { $detail }
angzarr-fmt-toml-needs-table = TOML output needs a table at the top level, not a { $found }
angzarr-fmt-toml-null = TOML has no null: the value at { $path } is empty
angzarr-fmt-toml-encode = the result could not be written as TOML: { $detail }
angzarr-fmt-write = the output could not be written: { $detail }
angzarr-fmt-catalog-parse = a message catalog has a syntax error: { $detail }

## Remedies.

angzarr-fmt-use-json-or-yaml = use --format json or --format yaml for this output
