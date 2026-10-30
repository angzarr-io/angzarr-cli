# Working rules

## Docs and code state the current design only

Version control holds the history; the docs and code do not. When a decision
changes, rewrite the text in place to say what is true now. No "previously",
"was X, now Y", dated decision notes, or sections kept for reference. A
rejected alternative may stay only as rationale ("not X, because ...").

## Hexagonal

The core is pure: no I/O, no clap, no tonic, no file system. Use cases sit
behind ports (traits); adapters implement them. Results are data: a command
returns one value and `angzarr-fmt` decides how it is written, so the output
format is never a branch in business logic. `angzarr-fmt` knows nothing
about angzarr or about clap; angzarr-cli's `output` module is the only place
joining them.

## Errors are localizable: a code, args, and an en-US message

Every error and diagnostic implements `angzarr_embedded_diagnostic::Diagnostic` and
keeps an English `Display` as the fallback.

- Codes are stable and never reused: kebab-case, starting with the crate's
  name (`angzarr-fmt-unknown-format`). A code whose meaning changes gets a new
  code.
- Args carry data, never pre-formatted English.
- A new code needs an en-US message in `crates/<crate>/locales/en-US/<crate>.ftl`
  using exactly its args, and a sample in the crate's `diagnostic_samples()`;
  the catalog test (`angzarr_fmt::check`) fails otherwise.
- A failure that the user can act on names its remedy, itself a coded
  message.

## Unicode in source is visible

The workspace denies `invisible_characters`, `unicode_not_nfc` and
`non_ascii_literal`; every crate opts in with `[lints] workspace = true`.
Write a non-ASCII character in a literal as an escape.

## Use `just`

`just test` and `just lint` are the checks; both must pass before a commit.
