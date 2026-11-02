# Messages for this crate's diagnostics. Each id is a diagnostic's code
# without the crate's name, which the build prefixes
# (angzarr_diagnostic::build); the variables are its args.

unknown-format = unknown format { $name }: use json or text
serialize = the result could not be serialized: { $detail }
write = the output could not be written: { $detail }
catalog-parse = a message catalog has a syntax error: { $detail }
