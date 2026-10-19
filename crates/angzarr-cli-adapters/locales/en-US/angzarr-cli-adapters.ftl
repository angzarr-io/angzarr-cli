# Messages for angzarr-cli-adapters' diagnostics. Each id is a diagnostic's
# code; the variables are its args.

# $input is the image path as given, or - for stdin. $kind is an I/O error
# kind as a kebab-case token (not-found, permission-denied, ...).
angzarr-cli-adapters-read = { $input } could not be read: { $kind }
angzarr-cli-adapters-stdin-terminal = no image was given and stdin is a terminal
# $detail is the decoder's own English text: data to show as read, not to
# translate.
angzarr-cli-adapters-decode = { $input } is not a valid descriptor set: { $detail }

## Project settings. $file is a project file's path; $kind as above; $cause
## is annals-yaml's reason, a nested diagnostic rendered in its own words.

angzarr-cli-adapters-project-read = { $file } could not be read: { $kind }
angzarr-cli-adapters-project-write = { $file } could not be written: { $kind }
angzarr-cli-adapters-project-too-new = { $file } was written by a newer angzarr: { $cause }
angzarr-cli-adapters-project-load = { $file } is not valid: { $cause }

## Remedies.

angzarr-cli-adapters-fix-image = pass a binary descriptor set or buf image, as a path or on stdin: buf build -o image.binpb, or protoc --include_imports --descriptor_set_out=image.binpb
angzarr-cli-adapters-fix-upgrade = upgrade angzarr to a build that reads this version
